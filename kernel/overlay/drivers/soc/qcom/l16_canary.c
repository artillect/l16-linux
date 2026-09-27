// SPDX-License-Identifier: GPL-2.0-only
/*
 * Light L16 mainline bring-up canary.
 *
 * The L16 has no reachable UART and the power-key reset loses RAM, so the
 * only reliable signal from an early boot is whether the board resets itself.
 *
 * - A PS_HOLD restart handler at the highest priority, so a panic always
 *   resets the board (PSCI SYSTEM_RESET runs first otherwise, and is untested
 *   on this TrustZone).
 * - "l16canary=<stage>" resets through PS_HOLD when boot reaches <stage>
 *   (see include/linux/l16_canary.h). The board rebooting means the kernel got
 *   there; staying on the boot logo means it died earlier.
 */

#include <linux/delay.h>
#include <linux/device.h>
#include <linux/hrtimer.h>
#include <linux/smp.h>
#include <linux/percpu.h>
#include <linux/cpumask.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/kernel.h>
#include <linux/moduleparam.h>
#include <linux/l16_canary.h>
#include <linux/notifier.h>
#include <linux/reboot.h>
#include <linux/serial_core.h>
#include <asm/early_ioremap.h>

#define L16_PSHOLD	0x004ab000

/* APSS watchdog (stock DT qcom,wdt@9830000), 32 kHz sleep clock */
#define L16_WDT_BASE	0x09830000
#define WDT0_RST	0x04
#define WDT0_EN		0x08
#define WDT0_BARK_TIME	0x10
#define WDT0_BITE_TIME	0x14
#define WDT_HZ		32765

static int canary_stage = -1;
static int wdt_seconds;

static int __init l16canary_setup(char *str)
{
	return kstrtoint(str, 0, &canary_stage) ? -EINVAL : 0;
}
early_param("l16canary", l16canary_setup);

/*
 * "l16wdt=<seconds>": arm the watchdog in setup_arch and never pet it, so any
 * hang (even with interrupts off) ends in a watchdog bite and a warm reset,
 * which keeps RAM and the early console log below.
 */
static int __init l16wdt_setup(char *str)
{
	return kstrtoint(str, 0, &wdt_seconds) ? -EINVAL : 0;
}
early_param("l16wdt", l16wdt_setup);

/*
 * "l16reset=<seconds>": once SMP is up, a timer pinned to CPU3 resets the board
 * through PS_HOLD after <seconds>. Catches a hang on another CPU without the
 * hardware watchdog (whose bite TrustZone may turn into download mode).
 */
static int reset_seconds;
static void __iomem *pshold;
static DEFINE_PER_CPU(struct hrtimer, reset_timer);

static int __init l16reset_setup(char *str)
{
	return kstrtoint(str, 0, &reset_seconds) ? -EINVAL : 0;
}
early_param("l16reset", l16reset_setup);

static enum hrtimer_restart l16_reset_timer_fn(struct hrtimer *t)
{
	pr_emerg("l16_canary: reset timer fired on CPU%d\n", smp_processor_id());
	writel(0, pshold);
	return HRTIMER_NORESTART;
}

static void l16_start_reset_timer(void *unused)
{
	hrtimer_start(this_cpu_ptr(&reset_timer), ktime_set(reset_seconds, 0),
		      HRTIMER_MODE_REL_PINNED);
}

/* One timer per other online CPU, started without waiting, so a stuck CPU can't block boot */
static void __init l16_arm_reset_timer(void)
{
	int cpu;

	pshold = ioremap(L16_PSHOLD, sizeof(u32));
	if (!pshold)
		return;
	pr_info("l16_canary: reset timers (%d s) on online CPUs %*pbl\n",
		reset_seconds, cpumask_pr_args(cpu_online_mask));
	for_each_online_cpu(cpu) {
		hrtimer_setup(per_cpu_ptr(&reset_timer, cpu), l16_reset_timer_fn,
			      CLOCK_MONOTONIC, HRTIMER_MODE_REL_PINNED);
		if (cpu != smp_processor_id())
			smp_call_function_single(cpu, l16_start_reset_timer, NULL, 0);
	}
}

/* echo 1 > /sys/module/l16_canary/parameters/disarm: cancel the reset timers */
static int l16_disarm_set(const char *val, const struct kernel_param *kp)
{
	int cpu;

	if (!pshold)
		return 0;
	for_each_possible_cpu(cpu)
		hrtimer_cancel(per_cpu_ptr(&reset_timer, cpu));
	pr_info("l16_canary: reset timers disarmed\n");
	return 0;
}

static const struct kernel_param_ops l16_disarm_ops = {
	.set = l16_disarm_set,
};
module_param_cb(disarm, &l16_disarm_ops, NULL, 0200);

/* echo <seconds> > /sys/module/l16_canary/parameters/arm: (re)arm the reset timers */
static void l16_rearm_one(void *secs)
{
	hrtimer_start(this_cpu_ptr(&reset_timer), ktime_set(*(int *)secs, 0),
		      HRTIMER_MODE_REL_PINNED);
}

static int l16_arm_set(const char *val, const struct kernel_param *kp)
{
	static int secs;
	int cpu, ret;

	ret = kstrtoint(val, 0, &secs);
	if (ret || secs <= 0 || !pshold)
		return ret ? ret : -EINVAL;
	for_each_online_cpu(cpu) {
		hrtimer_cancel(per_cpu_ptr(&reset_timer, cpu));
		if (cpu != smp_processor_id())
			smp_call_function_single(cpu, l16_rearm_one, &secs, 1);
	}
	pr_info("l16_canary: reset timers re-armed (%d s)\n", secs);
	return 0;
}

static const struct kernel_param_ops l16_arm_ops = {
	.set = l16_arm_set,
};
module_param_cb(arm, &l16_arm_ops, NULL, 0200);

static void __init l16_arm_watchdog(void)
{
	void __iomem *wdt = early_ioremap(L16_WDT_BASE, 0x20);

	if (!wdt)
		return;
	writel(0, wdt + WDT0_EN);
	writel(1, wdt + WDT0_RST);
	/* bark after the bite, so only the bite (a reset) ever happens */
	writel((wdt_seconds + 1) * WDT_HZ, wdt + WDT0_BARK_TIME);
	writel(wdt_seconds * WDT_HZ, wdt + WDT0_BITE_TIME);
	writel(1, wdt + WDT0_EN);
	mb();
	early_iounmap(wdt, 0x20);
}

/*
 * "earlycon=l16ram,0x91bbe000": an early console that writes straight into
 * the stock kernel's pstore console zone (ramoops at 0x91b00000; the console
 * zone follows 0xbe000 of dump records). It uses the persistent_ram header
 * the stock 3.18 ramoops reads, as a ring over the first page (the earlycon
 * fixmap maps one page), so stock shows the last ~4 KB as console-ramoops.
 */
#define PERSISTENT_RAM_SIG	0x43474244
#define L16RAM_RING		(PAGE_SIZE - 12)

struct l16ram_buf {
	u32 sig;
	u32 start;
	u32 size;
	u8 data[];
};

static void l16ram_write(struct console *con, const char *s, unsigned int n)
{
	struct earlycon_device *dev = con->data;
	struct l16ram_buf __iomem *b = (struct l16ram_buf __iomem *)dev->port.membase;
	u32 start = readl(&b->start), size = readl(&b->size);

	while (n--) {
		writeb(*s++, &b->data[start]);
		start = (start + 1) % L16RAM_RING;
		if (size < L16RAM_RING)
			size++;
	}
	writel(start, &b->start);
	writel(size, &b->size);
}

static int __init l16ram_setup(struct earlycon_device *dev, const char *opt)
{
	struct l16ram_buf __iomem *b = (struct l16ram_buf __iomem *)dev->port.membase;

	if (!b)
		return -ENODEV;
	writel(PERSISTENT_RAM_SIG, &b->sig);
	writel(0, &b->start);
	writel(0, &b->size);
	dev->con->write = l16ram_write;
	return 0;
}
EARLYCON_DECLARE(l16ram, l16ram_setup);

static void __ref __noreturn l16_reset(bool early)
{
	void __iomem *p;

	if (early)
		p = early_ioremap(L16_PSHOLD, sizeof(u32));
	else
		p = ioremap(L16_PSHOLD, sizeof(u32));
	if (p) {
		writel(0, p);
		mb();
	}
	for (;;)
		mdelay(100);
}

static int l16_restart_handler(struct notifier_block *nb, unsigned long mode, void *cmd)
{
	l16_reset(false);
	return NOTIFY_DONE;
}

static struct notifier_block l16_restart_nb = {
	.notifier_call = l16_restart_handler,
	.priority = 255,
};

void __ref l16_canary_stage(int stage)
{
	if (stage == L16_CANARY_SETUP_ARCH && wdt_seconds > 0)
		l16_arm_watchdog();
	if (stage == L16_CANARY_START_KERNEL)
		register_restart_handler(&l16_restart_nb);
	/* initcall level 0 runs after smp_init(), so CPU3 is online */
	if (stage == L16_CANARY_INITCALL(0) && reset_seconds > 0)
		l16_arm_reset_timer();

	if (stage == canary_stage) {
		pr_emerg("l16_canary: reached stage %d, resetting\n", stage);
		l16_reset(stage == L16_CANARY_SETUP_ARCH);
	}
}

void __ref l16_canary_initcall(void *fn)
{
	static int n;

	if (canary_stage > L16_CANARY_NTH_INITCALL && ++n == canary_stage - L16_CANARY_NTH_INITCALL) {
		pr_emerg("l16_canary: before initcall #%d %ps, resetting\n", n, fn);
		l16_reset(false);
	}
}

void l16_canary_probe(struct device *dev, const char *drv)
{
	static int n;

	if (canary_stage < L16_CANARY_NTH_PROBE)
		return;
	n++;
	pr_info("l16 probe #%d %s %s\n", n, drv, dev_name(dev));
	if (n == canary_stage - L16_CANARY_NTH_PROBE) {
		pr_emerg("l16_canary: before probe #%d, resetting\n", n);
		l16_reset(false);
	}
}
