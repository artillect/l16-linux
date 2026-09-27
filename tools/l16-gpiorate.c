// SPDX-License-Identifier: GPL-2.0-only
/*
 * l16-gpiorate: sample an MSM8996 TLMM pin's input level straight from its
 * GPIO_IN_OUT register (bit 0) and report how often it goes high, e.g. to see
 * whether the panel's TE signal on GPIO 10 is pulsing (works while the pin is
 * muxed to another function such as mdp_vsync).
 *
 * usage: l16-gpiorate <gpio> [seconds]
 */
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <time.h>

#define TLMM_BASE	0x01010000UL
#define TLMM_STRIDE	0x1000UL
#define IN_OUT		0x4

static double now(void)
{
	struct timespec t;

	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec + t.tv_nsec / 1e9;
}

int main(int argc, char **argv)
{
	int gpio = argc > 1 ? atoi(argv[1]) : 10;
	double secs = argc > 2 ? atof(argv[2]) : 2.0;
	unsigned long reg = TLMM_BASE + gpio * TLMM_STRIDE;
	long samples = 0, high = 0, edges = 0;
	int fd, prev = -1;
	volatile uint32_t *p;
	double t0, t;

	fd = open("/dev/mem", O_RDONLY | O_SYNC);
	if (fd < 0) {
		perror("/dev/mem");
		return 1;
	}
	p = mmap(NULL, 0x1000, PROT_READ, MAP_SHARED, fd, reg & ~0xfffUL);
	if (p == MAP_FAILED) {
		perror("mmap");
		return 1;
	}
	p += (reg & 0xfff) / 4 + IN_OUT / 4;

	t0 = now();
	do {
		for (int i = 0; i < 1000; i++) {
			int v = *p & 1;

			samples++;
			high += v;
			if (prev == 0 && v == 1)
				edges++;
			prev = v;
		}
		t = now();
	} while (t - t0 < secs);

	printf("gpio%d: %ld samples in %.2f s, high %.3f%%, %ld rising edges (%.1f Hz)\n",
	       gpio, samples, t - t0, 100.0 * high / samples, edges, edges / (t - t0));
	return 0;
}
