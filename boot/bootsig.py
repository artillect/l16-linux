#!/usr/bin/env python3
# Append an Android Verified Boot 1.0 boot signature to a boot image, as AOSP's
# boot_signer does. The L16's FIH LK needs a well-formed signature after the image
# (without one it ends up in 9008); unlocked, it does not care whose key signed it.
#
# usage: bootsig.py <boot.img> <key.pem> <cert.der>
#   key.pem  RSA private key (openssl genrsa 2048)
#   cert.der self-signed certificate for it (openssl req -new -x509 -outform DER)
import subprocess
import sys


def der(tag, body):
    n = len(body)
    if n < 0x80:
        length = bytes([n])
    else:
        b = n.to_bytes((n.bit_length() + 7) // 8, 'big')
        length = bytes([0x80 | len(b)]) + b
    return bytes([tag]) + length + body


def der_int(v):
    b = v.to_bytes(max(1, (v.bit_length() + 8) // 8), 'big')
    return der(0x02, b)


def der_seq(*items):
    return der(0x30, b''.join(items))


# sha256WithRSAEncryption, 1.2.840.113549.1.1.11
SHA256_RSA = der(0x06, bytes.fromhex('2a864886f70d01010b'))

image_path, key_path, cert_path = sys.argv[1:4]
image = open(image_path, 'rb').read()
cert = open(cert_path, 'rb').read()

auth_attrs = der_seq(der(0x13, b'/boot'), der_int(len(image)))
signature = subprocess.run(['openssl', 'dgst', '-sha256', '-sign', key_path],
                           input=image + auth_attrs, capture_output=True,
                           check=True).stdout

boot_signature = der_seq(der_int(1), cert, der_seq(SHA256_RSA, der(0x05, b'')),
                         auth_attrs, der(0x04, signature))

with open(image_path, 'ab') as f:
    f.write(boot_signature)
print('appended %d-byte boot signature to %s (%d bytes signed)'
      % (len(boot_signature), image_path, len(image)))
