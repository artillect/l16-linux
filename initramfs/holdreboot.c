// SPDX-License-Identifier: GPL-2.0-only
/*
 * holdreboot: reboot (back to Android) when the L16's power key is held for 3 seconds.
 * A short press does nothing. The PMIC's own long-press hard reset stays as a fallback.
 *
 * Finds the input device named "pm8941_pwrkey" and waits for KEY_POWER events.
 */
#include <dirent.h>
#include <fcntl.h>
#include <linux/input.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/reboot.h>
#include <time.h>
#include <unistd.h>

#define HOLD_MS 3000

static int open_pwrkey(void)
{
	char path[128], name[64];
	struct dirent *e;
	DIR *d = opendir("/sys/class/input");

	if (!d)
		return -1;
	while ((e = readdir(d))) {
		FILE *f;

		if (strncmp(e->d_name, "event", 5))
			continue;
		snprintf(path, sizeof(path), "/sys/class/input/%s/device/name", e->d_name);
		f = fopen(path, "r");
		if (!f)
			continue;
		if (fgets(name, sizeof(name), f) && !strncmp(name, "pm8941_pwrkey", 13)) {
			fclose(f);
			closedir(d);
			snprintf(path, sizeof(path), "/dev/input/%s", e->d_name);
			return open(path, O_RDONLY);
		}
		fclose(f);
	}
	closedir(d);
	return -1;
}

static long now_ms(void)
{
	struct timespec t;

	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec * 1000 + t.tv_nsec / 1000000;
}

static void kmsg(const char *s)
{
	int fd = open("/dev/kmsg", O_WRONLY);

	if (fd >= 0) {
		write(fd, s, strlen(s));
		close(fd);
	}
}

int main(void)
{
	struct input_event ev;
	long down = 0;
	int fd;

	for (int i = 0; i < 30 && (fd = open_pwrkey()) < 0; i++)
		sleep(1);
	if (fd < 0) {
		kmsg("holdreboot: no pm8941_pwrkey input device\n");
		return 1;
	}
	kmsg("holdreboot: hold the power key 3 s to reboot to Android\n");

	for (;;) {
		struct pollfd p = { .fd = fd, .events = POLLIN };
		int timeout = down ? (int)(HOLD_MS - (now_ms() - down)) : -1;

		if (down && timeout <= 0)
			timeout = 0;
		if (poll(&p, 1, timeout) == 0 && down) {
			kmsg("holdreboot: power key held, rebooting to Android\n");
			sync();
			reboot(RB_AUTOBOOT);
			return 0;
		}
		if (read(fd, &ev, sizeof(ev)) != sizeof(ev))
			continue;
		if (ev.type != EV_KEY || ev.code != KEY_POWER)
			continue;
		if (ev.value == 1)
			down = now_ms();
		else if (ev.value == 0)
			down = 0;
	}
}
