# Repack the Magisk boot.img with a new kernel: Image.gz + the LFC P2 PVT DTB appended.
# Keeps the original header (addresses, page size, cmdline) and the Magisk ramdisk untouched.
# usage: python repack.py <orig boot.img> <Image.gz> <dtb> <out.img> [extra cmdline]
#   extra cmdline: space-separated tokens to append; "-token" removes an existing token
import hashlib
import os
import shlex
import struct
import sys

NUL = bytes(1)

orig, image_gz, dtb, out = sys.argv[1:5]
extra = sys.argv[5] if len(sys.argv) > 5 else ''
d = open(orig, 'rb').read()
assert d[:8] == b'ANDROID!'
ksize, kaddr, rsize, raddr, ssize, saddr, tags, page = struct.unpack('<8I', d[8:40])


def pad(n):
    return (n + page - 1) // page * page


roff = page + pad(ksize)
ramdisk = d[roff:roff + rsize]
second = d[roff + pad(rsize):roff + pad(rsize) + ssize]
# RAMDISK=<file> replaces the ramdisk (used for mainline boots, which must not run Android's init)
if os.environ.get('RAMDISK'):
    ramdisk = open(os.environ['RAMDISK'], 'rb').read()

kernel = open(image_gz, 'rb').read() + open(dtb, 'rb').read()

sha = hashlib.sha1()
for blob in (kernel, ramdisk, second):
    sha.update(blob)
    sha.update(struct.pack('<I', len(blob)))

hdr = bytearray(d[:page])
if extra:
    # "-token" removes that token from the original cmdline; anything else is appended
    cmd = bytes(hdr[64:576]).split(NUL)[0].split(b' ')
    # shell-style splitting, so a quoted value (e.g. dyndbg="func x +p") stays one token
    for tok in (shlex.quote(t) if ' ' in t else t for t in shlex.split(extra)):
        tok = tok.replace("'", '"').encode()
        if tok.startswith(b'-'):
            cmd.remove(tok[1:])
        else:
            cmd.append(tok)
    cmd = b' '.join(cmd)
    assert len(cmd) < 512, 'cmdline too long'
    hdr[64:576] = cmd.ljust(512, NUL)
struct.pack_into('<I', hdr, 8, len(kernel))
struct.pack_into('<I', hdr, 16, len(ramdisk))
hdr[576:608] = sha.digest().ljust(32, NUL)


def padded(b):
    return b + NUL * (pad(len(b)) - len(b))


with open(out, 'wb') as f:
    f.write(hdr)
    f.write(padded(kernel))
    f.write(padded(ramdisk))
    if second:
        f.write(padded(second))
    # The L16 bootloader goes wrong (ends in 9008) when no boot signature follows the image.
    # Append the stock one; this bootloader is unlocked, so it only needs to be present.
    sig_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'stock-bootsig.bin')
    f.write(open(sig_path, 'rb').read())
    # the bootloader expects a full partition-sized image
    f.truncate(64 * 1024 * 1024)

print('kernel', len(kernel), 'ramdisk', len(ramdisk),
      'cmdline', bytes(hdr[64:576]).split(NUL)[0].decode())
