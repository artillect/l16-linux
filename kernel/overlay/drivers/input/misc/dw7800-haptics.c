// SPDX-License-Identifier: GPL-2.0-only
/*
 * Dongwoon DW7800 haptic driver (Light L16 vibration motor), as a memoryless
 * force-feedback rumble device. The chip plays signed 8-bit samples written into
 * a FIFO; this follows what the stock L16 kernel's dw7800 driver does:
 *
 *   power on   reg 5 = 1 (software reset), wait 1.1 ms,
 *              reg 6 = 0x20 (timing), reg 7 = 0x08 (LDO level), reg 8 = 0
 *   play       while on: when the FIFO level (reg 3) is <= 0x50, write 40 samples
 *              of a sine (20 samples per period) to the FIFO (reg 4)
 *   stop       one 0 sample, then power on again (empties the FIFO)
 *
 * Every register write is one transfer of [reg, data...]. Reg 1 reads 0x02.
 */
#include <linux/delay.h>
#include <linux/i2c.h>
#include <linux/input.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/workqueue.h>

#define DW7800_REG_VERSION	0x01
#define DW7800_REG_FIFO_LEVEL	0x03
#define DW7800_REG_FIFO		0x04
#define DW7800_REG_RESET	0x05
#define DW7800_REG_TIMING	0x06
#define DW7800_REG_LDO		0x07
#define DW7800_REG_HW_RESET	0x08

/* stock values */
#define DW7800_TIMING		0x20
#define DW7800_LDO		0x08
#define DW7800_FIFO_LOW		0x50
#define DW7800_CHUNK		40

/* one period of stock's normal_pattern */
static const s8 dw7800_sine[20] = {
	0, 20, 39, 58, 75, 90, 103, 114, 121, 126,
	127, 126, 121, 114, 103, 90, 75, 58, 39, 20,
};

struct dw7800 {
	struct i2c_client *client;
	struct input_dev *input;
	struct work_struct work;
	unsigned int level;	/* 0..0xffff, 0 = off */
};

static int dw7800_write(struct dw7800 *dw, u8 reg, u8 val)
{
	return i2c_smbus_write_byte_data(dw->client, reg, val);
}

static void dw7800_power_on(struct dw7800 *dw)
{
	dw7800_write(dw, DW7800_REG_RESET, 1);
	usleep_range(1100, 1200);
	dw7800_write(dw, DW7800_REG_TIMING, DW7800_TIMING);
	dw7800_write(dw, DW7800_REG_LDO, DW7800_LDO);
	dw7800_write(dw, DW7800_REG_HW_RESET, 0);
}

static void dw7800_work(struct work_struct *work)
{
	struct dw7800 *dw = container_of(work, struct dw7800, work);
	u8 buf[1 + DW7800_CHUNK];
	unsigned int level;
	int fill, i, ret;

	while ((level = READ_ONCE(dw->level))) {
		fill = i2c_smbus_read_byte_data(dw->client, DW7800_REG_FIFO_LEVEL);
		if (fill < 0)
			break;
		if (fill > DW7800_FIFO_LOW) {
			usleep_range(1000, 2000);
			continue;
		}
		/* the sine, then its negative half */
		buf[0] = DW7800_REG_FIFO;
		for (i = 0; i < DW7800_CHUNK; i++) {
			int s = dw7800_sine[i % 20] * (i < 20 ? 1 : -1);

			buf[1 + i] = (s8)(s * (int)level / 0xffff);
		}
		ret = i2c_master_send(dw->client, buf, sizeof(buf));
		if (ret < 0)
			break;
	}

	dw7800_write(dw, DW7800_REG_FIFO, 0);
	dw7800_power_on(dw);
}

static int dw7800_play(struct input_dev *input, void *data, struct ff_effect *effect)
{
	struct dw7800 *dw = input_get_drvdata(input);
	unsigned int level = max(effect->u.rumble.strong_magnitude,
				 effect->u.rumble.weak_magnitude / 2);

	WRITE_ONCE(dw->level, level);
	if (level)
		queue_work(system_long_wq, &dw->work);

	return 0;
}

static void dw7800_stop(struct dw7800 *dw)
{
	WRITE_ONCE(dw->level, 0);
	cancel_work_sync(&dw->work);
}

static void dw7800_close(struct input_dev *input)
{
	dw7800_stop(input_get_drvdata(input));
}

static void dw7800_cancel(void *data)
{
	dw7800_stop(data);
}

static int dw7800_probe(struct i2c_client *client)
{
	struct device *dev = &client->dev;
	struct dw7800 *dw;
	int ret;

	dw = devm_kzalloc(dev, sizeof(*dw), GFP_KERNEL);
	if (!dw)
		return -ENOMEM;
	dw->client = client;
	INIT_WORK(&dw->work, dw7800_work);
	i2c_set_clientdata(client, dw);

	ret = i2c_smbus_read_byte_data(client, DW7800_REG_VERSION);
	if (ret < 0)
		return dev_err_probe(dev, ret, "no response\n");
	dev_info(dev, "version %#x\n", ret);
	dw7800_power_on(dw);

	dw->input = devm_input_allocate_device(dev);
	if (!dw->input)
		return -ENOMEM;
	dw->input->name = "dw7800-haptics";
	dw->input->id.bustype = BUS_I2C;
	dw->input->close = dw7800_close;
	input_set_drvdata(dw->input, dw);
	input_set_capability(dw->input, EV_FF, FF_RUMBLE);

	ret = devm_add_action_or_reset(dev, dw7800_cancel, dw);
	if (ret)
		return ret;

	ret = input_ff_create_memless(dw->input, NULL, dw7800_play);
	if (ret)
		return ret;

	return input_register_device(dw->input);
}

static int dw7800_suspend(struct device *dev)
{
	dw7800_stop(dev_get_drvdata(dev));
	return 0;
}

static DEFINE_SIMPLE_DEV_PM_OPS(dw7800_pm, dw7800_suspend, NULL);

static const struct of_device_id dw7800_of_match[] = {
	{ .compatible = "dongwoon,dw7800" },
	{ }
};
MODULE_DEVICE_TABLE(of, dw7800_of_match);

static struct i2c_driver dw7800_driver = {
	.driver = {
		.name = "dw7800-haptics",
		.of_match_table = dw7800_of_match,
		.pm = pm_sleep_ptr(&dw7800_pm),
	},
	.probe = dw7800_probe,
};
module_i2c_driver(dw7800_driver);

MODULE_DESCRIPTION("Dongwoon DW7800 haptic driver");
MODULE_LICENSE("GPL");
