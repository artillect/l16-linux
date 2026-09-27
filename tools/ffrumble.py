# Play one rumble effect on an evdev force-feedback device.
# usage: ffrumble.py /dev/input/eventN <ms> [strength 0-65535]
import fcntl, struct, sys, time
dev, ms = sys.argv[1], int(sys.argv[2])
mag = int(sys.argv[3]) if len(sys.argv) > 3 else 0xffff
FF_RUMBLE, EV_FF = 0x50, 0x15
EVIOCSFF = 0x40304580  # _IOW('E', 0x80, struct ff_effect), 48 bytes on arm64
effect = bytearray(struct.pack('<HhHHHHH2x', FF_RUMBLE, -1, 0, 0, 0, ms, 0))
effect += struct.pack('<HH', mag, mag) + bytes(28)
fd = open(dev, 'r+b', buffering=0)
fcntl.ioctl(fd, EVIOCSFF, effect)
eid = struct.unpack_from('<h', effect, 2)[0]
fd.write(struct.pack('<qqHHi', 0, 0, EV_FF, eid, 1))
time.sleep(ms / 1000 + 0.2)
print('played effect', eid)
