#!/usr/bin/env python3
# l16-gpt.py: make room for Linux on the Light L16. Reads LUN a's partition table (GPT)
# from backups of the disk's first and last MiB, and writes new partition-table images
# that shrink "userdata" (the last partition) and add a "linux" partition in the freed
# space at the end. Nothing touches the camera: it prints the commands to write them.
#
# The images cover only the GPT itself: the protective MBR, the primary header and
# entries up to the first usable sector, and the backup entries and header at the end.
# Other small partitions share the first MiB, so writing back the whole MiB would roll
# them back.
#
# usage:
#   l16-gpt.py <head1M.img> <tail1M.img> <disk size in 512-byte sectors>
#       show the partition table
#   l16-gpt.py <head1M.img> <tail1M.img> <disk size in 512-byte sectors> --add-linux <GiB> <out dir>
#       write gpt-new-{primary,backup}.img (userdata shrunk by <GiB>, "linux" added) and
#       gpt-orig-{primary,backup}.img (the current table, to undo it)
import os
import struct
import sys
import uuid
import zlib

LINUX_FS = uuid.UUID('0fc63daf-8483-4772-8e79-3d69d8477de4')
HDR = '<8sIIIIQQQQ16sQIII'

head = bytearray(open(sys.argv[1], 'rb').read())
tail = bytearray(open(sys.argv[2], 'rb').read())
disk512 = int(sys.argv[3])

# the sector size is where the "EFI PART" header sits (4096 on the L16's UFS)
for ss in (4096, 512):
    if head[ss:ss + 8] == b'EFI PART':
        break
else:
    sys.exit('no GPT header in %s' % sys.argv[1])
disk_lbas = disk512 * 512 // ss
tail_first = disk_lbas - len(tail) // ss   # LBA of tail[0]


def parse_hdr(buf, off):
    f = struct.unpack_from(HDR, buf, off)
    return dict(sig=f[0], size=f[2], crc=f[3], my=f[5], alt=f[6], first=f[7], last=f[8],
                ents=f[10], n=f[11], esize=f[12], ecrc=f[13])


def at(lba):
    """(buffer, offset) holding LBA `lba`, from the head or the tail image."""
    if lba * ss < len(head):
        return head, lba * ss
    if lba >= tail_first:
        return tail, (lba - tail_first) * ss
    sys.exit('LBA %d is in neither image' % lba)


def hdr_ok(buf, off, h):
    c = bytearray(buf[off:off + h['size']])
    c[16:20] = b'\0' * 4
    return zlib.crc32(c) == h['crc']


p = parse_hdr(head, ss)
b = parse_hdr(*at(p['alt']))
if b['sig'] != b'EFI PART' or b['alt'] != p['my']:
    sys.exit('no backup GPT header at LBA %d: wrong disk size, or not the end of the disk?' % p['alt'])
elen = p['n'] * p['esize']
buf, off = at(p['ents'])
entries = bytearray(buf[off:off + elen])
buf, off = at(b['ents'])
if not (hdr_ok(head, ss, p) and hdr_ok(*at(p['alt']), b) and zlib.crc32(entries) == p['ecrc']
        and bytes(buf[off:off + elen]) == bytes(entries)):
    sys.exit('the partition table in these images is damaged or inconsistent; stop here')

parts = []
for i in range(p['n']):
    e = entries[i * p['esize']:(i + 1) * p['esize']]
    if e[:16] == b'\0' * 16:
        continue
    first, last = struct.unpack_from('<QQ', e, 32)
    parts.append((i, e[56:128].decode('utf-16-le').rstrip('\0'), first, last))

print('sector size %d, %d sectors' % (ss, disk_lbas))
for i, name, first, last in parts:
    print('  #%-2d %-12s %10d..%-10d %10.1f MiB' % (i, name, first, last, (last - first + 1) * ss / 2**20))

if '--add-linux' not in sys.argv:
    sys.exit(0)

a = sys.argv.index('--add-linux')
gib, out = int(sys.argv[a + 1]), sys.argv[a + 2]
names = [x[1] for x in parts]
if 'linux' in names:
    sys.exit('there is a "linux" partition already')
i, name, first, last = max(parts, key=lambda x: x[3])
if name != 'userdata':
    sys.exit('the last partition is "%s", not "userdata"; this is not the layout this tool knows' % name)
shrink = gib * 2**30 // ss
if (last - first + 1) - shrink < 8 * 2**30 // ss:
    sys.exit('that would leave userdata with less than 8 GiB')
free = next((k for k in range(p['n']) if entries[k * p['esize']:k * p['esize'] + 16] == b'\0' * 16), None)
if free is None:
    sys.exit('no free partition table entry')

new = bytearray(entries)
struct.pack_into('<Q', new, i * p['esize'] + 40, last - shrink)
e = bytearray(p['esize'])
e[0:16] = LINUX_FS.bytes_le
e[16:32] = uuid.uuid4().bytes_le
struct.pack_into('<QQ', e, 32, last - shrink + 1, last)
e[56:66] = 'linux'.encode('utf-16-le')
new[free * p['esize']:(free + 1) * p['esize']] = e
ecrc = zlib.crc32(new)


def with_table(h, my_hdr):
    """The header `h` rewritten for the new entries."""
    buf, off = at(my_hdr)
    hb = bytearray(buf[off:off + ss])
    struct.pack_into('<I', hb, 88, ecrc)
    hb[16:20] = b'\0' * 4
    struct.pack_into('<I', hb, 16, zlib.crc32(hb[:h['size']]))
    return hb


def region(lo, hi, table, hdrs):
    """LBAs lo..hi, with the entries and headers replaced if given."""
    img = bytearray()
    for lba in range(lo, hi + 1):
        buf, off = at(lba)
        img += buf[off:off + ss]
    if table is not None:
        for ents_lba in (p['ents'], b['ents']):
            if lo <= ents_lba <= hi:
                o = (ents_lba - lo) * ss
                img[o:o + elen] = table
        for lba, hb in hdrs:
            if lo <= lba <= hi:
                img[(lba - lo) * ss:(lba - lo + 1) * ss] = hb
    return img


# primary: LBA 0 up to the first usable sector; backup: its entries to the end of the disk
prim_hi = p['first'] - 1
back_lo = b['ents']
hdrs = [(p['my'], with_table(p, p['my'])), (b['my'], with_table(b, b['my']))]
os.makedirs(out, exist_ok=True)
files = {
    'gpt-new-primary.img': region(0, prim_hi, new, hdrs),
    'gpt-new-backup.img': region(back_lo, disk_lbas - 1, new, hdrs),
    'gpt-orig-primary.img': region(0, prim_hi, None, []),
    'gpt-orig-backup.img': region(back_lo, disk_lbas - 1, None, []),
}
for f, img in files.items():
    open(os.path.join(out, f), 'wb').write(img)

nl = last - shrink
print('\nuserdata: %d..%d -> %d..%d' % (first, last, first, nl))
print('linux:    %d..%d (%d GiB)' % (nl + 1, last, gib))
print('\nwrote %s/gpt-{new,orig}-{primary,backup}.img' % out)
print('\nwrite the new table (as root, one command per su call):')
for which, seek in (('primary', 0), ('backup', back_lo)):
    print('  dd if=/data/local/tmp/gpt-new-%s.img of=/dev/block/sda bs=%d seek=%d conv=fsync'
          % (which, ss, seek))
print('to undo it, write gpt-orig-*.img the same way.')
