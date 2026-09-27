/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef __L16_CANARY_H__
#define __L16_CANARY_H__

/* Boot stages for "l16canary=<stage>" */
#define L16_CANARY_SETUP_ARCH		0	/* setup_arch, after early params */
#define L16_CANARY_START_KERNEL		1	/* start_kernel, before rest_init */
#define L16_CANARY_INITCALL(level)	(10 + (level))	/* before initcall level 0..7 */
#define L16_CANARY_INITCALLS_DONE	18	/* all initcalls done, before rootfs */

/* l16canary=1000+N resets before the Nth initcall (counting early initcalls) */
#define L16_CANARY_NTH_INITCALL		1000
/* l16canary=100000+N logs driver probes and resets before the Nth */
#define L16_CANARY_NTH_PROBE		100000

struct device;

#ifdef CONFIG_L16_CANARY
void l16_canary_stage(int stage);
void l16_canary_initcall(void *fn);
void l16_canary_probe(struct device *dev, const char *drv);
#else
static inline void l16_canary_stage(int stage) { }
static inline void l16_canary_initcall(void *fn) { }
static inline void l16_canary_probe(struct device *dev, const char *drv) { }
#endif

#endif
