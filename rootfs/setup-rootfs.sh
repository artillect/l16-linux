#!/bin/sh
# Configure the postmarketOS rootfs on the L16's "linux" partition (mounted at $R).
# Run on the phone from the initramfs shell after `apk --root $R add ...`.
set -e
R=${R:-/mnt}
U=riley

echo l16 > $R/etc/hostname
# Phosh treats our DT chassis-type "embedded" as docked (floating windows, no automatic
# on-screen keyboard). A tablet is only docked with a pointer attached, and Phosh's
# lock-screen portrait forcing only applies to phones.
grep -q '^CHASSIS=' $R/etc/machine-info 2>/dev/null || echo 'CHASSIS=tablet' >> $R/etc/machine-info
printf '127.0.0.1\tlocalhost l16\n::1\t\tlocalhost l16\n' > $R/etc/hosts
echo 'LABEL=pmOS_root	/	ext4	rw,relatime	0 1' > $R/etc/fstab

# user (no password yet: set it over ssh with `passwd riley`), same ssh keys as the initramfs
chroot $R adduser -D -s /bin/ash $U 2>/dev/null || true
for g in wheel audio video input plugdev netdev feedbackd; do
	chroot $R addgroup $U $g 2>/dev/null || true
done
for h in /root /home/$U; do
	mkdir -p $R$h/.ssh
	cp /root/.ssh/authorized_keys $R$h/.ssh/
	chmod 700 $R$h/.ssh; chmod 600 $R$h/.ssh/authorized_keys
done
chroot $R chown -R $U:$U /home/$U/.ssh

# ssh is key-only: the user password is a short PIN for the lock screen
printf '# key-only: the user password is a short PIN for the lock screen\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n' \
	> $R/etc/ssh/sshd_config.d/50-l16-keyonly.conf
# key-authenticated root may open a reverse tunnel (package mirrors through the PC:
# ssh -R 3128:127.0.0.1:3128 with pmos/proxy.py). Match blocks must end the file.
grep -q '^Match User root' $R/etc/ssh/sshd_config ||
	printf '\nMatch User root\n\tAllowTcpForwarding remote\n' >> $R/etc/ssh/sshd_config

# USB network: our initramfs sets up the NCM gadget; NetworkManager gives it
# 172.16.42.1 and hands the PC an address by DHCP ("shared" mode, dnsmasq)
mkdir -p $R/etc/NetworkManager/system-connections
cat > $R/etc/NetworkManager/system-connections/usb0.nmconnection <<EOF
[connection]
id=USB network
type=ethernet
interface-name=usb0
autoconnect=true

[ipv4]
method=shared
address1=172.16.42.1/24

[ipv6]
method=disabled
EOF
chmod 600 $R/etc/NetworkManager/system-connections/usb0.nmconnection

# touchscreen: keep bezel swipes (raw X = 0, the landscape bottom edge) on screen
cp "$(dirname "$0")/90-l16-touchscreen.rules" $R/etc/udev/rules.d/

# Audio: UCM profile for the "Light L16" card (apq8096 driver): speaker on LINEOUT1
mkdir -p $R/usr/share/alsa/ucm2/Light/L16
cp "$(dirname "$0")/ucm/Light/L16/L16.conf" "$(dirname "$0")/ucm/Light/L16/HiFi.conf" $R/usr/share/alsa/ucm2/Light/L16/
ln -sf ../../Light/L16/L16.conf "$R/usr/share/alsa/ucm2/conf.d/apq8096/Light L16.conf"
# front and rear mics as separate mono inputs besides the stereo one
mkdir -p $R/etc/pulse/default.pa.d
cp "$(dirname "$0")/pulse/l16-mics.pa" $R/etc/pulse/default.pa.d/
# The login screen's PulseAudio kept the sound card open after login, leaving the
# user's PulseAudio with only a dummy output: no PulseAudio for the greeter user.
G=$R/var/lib/greetd
mkdir -p $G/.config/autostart $G/.config/pulse
printf '[Desktop Entry]\nType=Application\nName=PulseAudio Sound System\nHidden=true\n' > $G/.config/autostart/pulseaudio.desktop
printf 'autospawn = no\n' > $G/.config/pulse/client.conf
chroot $R chown -R greetd:greetd /var/lib/greetd/.config

# vibration motor: feedbackd only uses input devices its udev rules tag as "vibra"
cp "$(dirname "$0")/72-l16-haptics.rules" $R/etc/udev/rules.d/

# serial shell on the USB ACM port as well
grep -q ttyGS0 $R/etc/inittab || echo 'ttyGS0::respawn:/sbin/getty -L 115200 ttyGS0 vt100' >> $R/etc/inittab
grep -q '^ttyGS0' $R/etc/securetty 2>/dev/null || echo ttyGS0 >> $R/etc/securetty

# Phosh and the phrog login screen: the L16 is a landscape camera with a natively
# portrait panel. The kernel reports it (DT rotation = <90>, "Right Side Up"); tell phoc
# to use that (phoc's own key would be "rotate", not "transform"). 1.75x keeps the lock
# screen keypad on screen in landscape.
mkdir -p $R/etc/phosh $R/etc/phrog
cat > $R/etc/phosh/phoc.ini <<EOF
[output:DSI-1]
# rotation comes from the kernel panel orientation (DT rotation = <90>)
drm-panel-orientation = true
scale = 1.75
EOF
cp $R/etc/phosh/phoc.ini $R/etc/phrog/phoc.ini

# Phosh forces portrait when it starts locked before its async chassis lookup returns
# (device type defaults to "phone"); the phrog login screen always hits that. Put the
# login screen back in landscape once it has happened.
mkdir -p $R/etc/phrog/autostart $R/usr/local/libexec
cat > $R/usr/local/libexec/l16-greeter-rotate <<'EOF'
#!/bin/sh
for i in $(seq 1 40); do
	if wlr-randr | grep -q "Transform: normal"; then
		wlr-randr --output DSI-1 --transform 270
		exit 0
	fi
	sleep 0.5
done
EOF
chmod 755 $R/usr/local/libexec/l16-greeter-rotate
printf '[Desktop Entry]\nType=Application\nName=L16 landscape login screen\nExec=/usr/local/libexec/l16-greeter-rotate\nNoDisplay=true\n' \
	> $R/etc/phrog/autostart/l16-greeter-rotate.desktop

# Adreno 530 firmware, copied from the stock system partition (the zap shader is signed
# for this device). Needs the stock system partition mounted read-only at $STOCK.
STOCK=${STOCK:-/tmp/sys}
F=$STOCK/etc/firmware
mkdir -p $R/lib/firmware/qcom/apq8096/l16
cp $F/a530_pm4.fw $F/a530_pfp.fw $F/a530v3_gpmu.fw2 $R/lib/firmware/qcom/
cp $F/a530_zap.mdt $F/a530_zap.b00 $F/a530_zap.b01 $F/a530_zap.b02 $R/lib/firmware/qcom/apq8096/l16/

# Wi-Fi board data: stock reads board_id 0 from the QCA6174 OTP, finds no bdwlan30.b00
# and loads the default bdwlan30.bin (modem partition image/). ath10k falls back to
# board.bin (the card has no subsystem IDs); /lib/firmware/updates wins over the
# linux-firmware-ath10k copy. The generic one gives bad reception and no 2.4 GHz.
mkdir -p $R/lib/firmware/updates/ath10k/QCA6174/hw3.0
cp "$(dirname "$0")/wlan-stock/bdwlan30.bin" $R/lib/firmware/updates/ath10k/QCA6174/hw3.0/board.bin

# Bluetooth (QCA6174 "ROME" 3.2): stock's firmware from /system/etc/firmware under the
# names mainline's hci_qca asks for (controller version 0x00440302)
mkdir -p $R/lib/firmware/updates/qca
cp "$(dirname "$0")/bt-stock/rampatch_tlv_3.2.tlv" $R/lib/firmware/updates/qca/rampatch_00440302.bin
cp "$(dirname "$0")/bt-stock/nvm_tlv_3.2.bin" $R/lib/firmware/updates/qca/nvm_00440302.bin

# Sensor DSP (SLPI): its firmware from the stock modem partition (image/slpi.*) and the
# sensor registry from stock persist (sensors/sns.reg, served by qcom_sns_reg). Needs
# modem and persist mounted read-only at $MODEM and $PERSIST.
MODEM=${MODEM:-/tmp/modem}
PERSIST=${PERSIST:-/tmp/persist}
mkdir -p $R/lib/firmware/qcom/sensors
cp $MODEM/image/slpi.* $MODEM/image/adsp.* $R/lib/firmware/qcom/apq8096/l16/
cp $PERSIST/sensors/sns.reg $R/lib/firmware/qcom/sensors/
# accelerometer mount matrix for iio-sensor-proxy (upright landscape = "normal")
cp "$(dirname "$0")/61-l16-sensors.rules" $R/etc/udev/rules.d/

# phosh-session/phrog only log through systemd-cat; OpenRC has none, so provide a
# minimal one (exec's the command; output to syslog through a FIFO)
mkdir -p $R/usr/local/bin
cp "$(dirname "$0")/systemd-cat" $R/usr/local/bin/systemd-cat && chmod 755 $R/usr/local/bin/systemd-cat

# greetd gives its sessions a clean environment; let /etc/environment through (pam_env)
grep -q pam_env $R/etc/pam.d/greetd || echo 'session optional pam_env.so readenv=1' >> $R/etc/pam.d/greetd

# the on-screen keyboard is off by default
# and Phosh's power menu has no Suspend entry by default (its schema is still sm.puri.phosh)
# wallpaper: stock LightOS lens-layout image (PartnerLayout.apk light_wallpaper.jpg) for
# home, lock screen and the phrog greeter; :Phosh sections beat pmOS's own :Phosh overrides
mkdir -p $R/usr/share/backgrounds/l16
cp "$(dirname "$0")/light_wallpaper.jpg" $R/usr/share/backgrounds/l16/
W=file:///usr/share/backgrounds/l16/light_wallpaper.jpg
printf '[org.gnome.desktop.a11y.applications]\nscreen-keyboard-enabled=true\n\n[sm.puri.phosh]\nenable-suspend=true\n' \
	> $R/usr/share/glib-2.0/schemas/90_l16.gschema.override
printf '\n[org.gnome.desktop.background:Phosh]\npicture-uri=%s\npicture-uri-dark=%s\n\n[org.gnome.desktop.screensaver:Phosh]\npicture-uri=%s\n' \
	"'$W'" "'$W'" "'$W'" >> $R/usr/share/glib-2.0/schemas/90_l16.gschema.override
chroot $R glib-compile-schemas /usr/share/glib-2.0/schemas 2>/dev/null

# Clock: the PMIC RTC is read-only (as on stock) and counts from ~1970. hwclock must not
# copy it into the system clock; l16-rtc-offset sets the clock from RTC + a saved offset
# at boot and saves the offset at shutdown; chrony (NTP) keeps it right when online.
# chrony's own rtcfile mode needs RTC update interrupts, which the PM8xxx RTC lacks.
cp "$(dirname "$0")/l16-rtc-offset.initd" $R/etc/init.d/l16-rtc-offset
chmod 755 $R/etc/init.d/l16-rtc-offset
printf '\nclock_hctosys="NO"\nclock_systohc="NO"\n' >> $R/etc/conf.d/hwclock
sed -i '/^rtcsync$/d' $R/etc/chrony/chrony.conf
chroot $R rc-update add l16-rtc-offset boot
chroot $R rc-update add chronyd default

# Suspend: Phosh only releases its logind delay lock once the screen has blanked, so a
# suspend with the screen on waited logind's full 5 s default (and a power press in that
# window raced the suspend). NetworkManager/UPower finish well within 1 s.
mkdir -p $R/etc/elogind/logind.conf.d
printf '[Login]\nInhibitDelayMaxSec=1\n' > $R/etc/elogind/logind.conf.d/50-l16-inhibit-delay.conf
# no modem on the APQ8096 (ModemManager also holds a suspend delay lock)
chroot $R rc-update del modemmanager default 2>/dev/null || true

# Phone-like sleep: suspend 3 s after the screen blanks (power button or idle), on
# battery only. Suspending with the screen already blank is fast and wakes cleanly.
cp "$(dirname "$0")/l16-suspend-on-blank" $R/usr/local/bin/ && chmod 755 $R/usr/local/bin/l16-suspend-on-blank
cp "$(dirname "$0")/l16-suspend-on-blank.desktop" $R/etc/xdg/autostart/
cp "$(dirname "$0")/l16-suspend-on-blank.desktop" $R/etc/phrog/autostart/
# it re-arms the RTC when a press lands while a suspend is starting (group wheel)
cp "$(dirname "$0")/90-l16-rtc-wakealarm.rules" $R/etc/udev/rules.d/
# the user at the device may suspend even while other sessions exist (e.g. ssh)
cp "$(dirname "$0")/50-l16-suspend.rules" $R/etc/polkit-1/rules.d/

# reboot straight back into Linux (plain reboot goes to Android)
mkdir -p $R/usr/local/sbin
cp /tmp/reboot-linux $R/usr/local/sbin/ && chmod 755 $R/usr/local/sbin/reboot-linux

# The L16 has no RTC in our DT yet, so the clock starts at 1970 and apk records the
# accounts' last password change as day 0 = "must change password now", which makes
# PAM refuse greetd's greeter. Set it to the install date.
sed -i -E "s/^([^:]*:[^:]*):0:/\1:$(( $(date +%s) / 86400 )):/" $R/etc/shadow

# sshd, networkmanager, dbus, elogind and greetd are already enabled by their packages
chroot $R rc-update show default
