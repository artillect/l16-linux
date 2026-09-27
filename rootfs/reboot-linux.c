// SPDX-License-Identifier: GPL-2.0-only
/*
 * reboot-linux: reboot the L16 straight back into Linux. The PMIC reboot reason
 * "recovery" makes FIH's LK boot the recovery partition, which holds our kernel.
 * Plain `reboot` goes to Android.
 */
#include <linux/reboot.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void)
{
	sync();
	return syscall(SYS_reboot, LINUX_REBOOT_MAGIC1, LINUX_REBOOT_MAGIC2,
		       LINUX_REBOOT_CMD_RESTART2, "recovery");
}
