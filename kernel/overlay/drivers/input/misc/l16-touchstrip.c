// SPDX-License-Identifier: GPL-2.0-only
/*
 * Touch strip of the Light L16: an Elan eKTF controller reporting one finger on a
 * one-dimensional strip. Protocol as seen on the wire (and as stock's elan-ktf
 * driver handles it):
 *
 *   after reset   55 55 55 55                  hello
 *   touch         5a b1 b2 b3 b4 b5 b6 bits    Elan 2-finger report, 8 bytes
 *                   finger 1 x = (b1 & 0xf0) << 4 | b2, 0..767 end to end
 *                   (y is always 0), bits = finger-down mask, 0 on release
 *
 * Info queries (53 60 00 00 -> 52 60 00 31) give an X resolution of 768 and a
 * Y resolution of 1. Other packets (hello, recalibration) are read and ignored;
 * every interrupt reads one packet, which releases the level-low IRQ line.
 */
#include <linux/delay.h>
#include <linux/gpio/consumer.h>
#include <linux/i2c.h>
#include <linux/input.h>
#include <linux/interrupt.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/regulator/consumer.h>

#define STRIP_PACKET_LEN	8
#define STRIP_HDR_2FINGER	0x5a
#define STRIP_X_MAX		767
/* give up on the IRQ after this many failed reads in a row, instead of spinning */
#define STRIP_MAX_ERRORS	20

struct l16_strip {
	struct i2c_client *client;
	struct input_dev *input;
	struct gpio_desc *reset_gpio;
	unsigned int errors;
};

static irqreturn_t l16_strip_irq(int irq, void *data)
{
	struct l16_strip *strip = data;
	u8 buf[STRIP_PACKET_LEN];
	int ret;

	ret = i2c_master_recv(strip->client, buf, sizeof(buf));
	if (ret != sizeof(buf)) {
		if (++strip->errors >= STRIP_MAX_ERRORS) {
			dev_err(&strip->client->dev,
				"%u failed reads in a row, disabling the interrupt\n",
				strip->errors);
			disable_irq_nosync(irq);
		}
		return IRQ_HANDLED;
	}
	strip->errors = 0;

	if (buf[0] != STRIP_HDR_2FINGER)
		return IRQ_HANDLED;

	if (buf[7] & 0x01) {
		input_report_abs(strip->input, ABS_X, ((buf[1] & 0xf0) << 4) | buf[2]);
		input_report_key(strip->input, BTN_TOUCH, 1);
	} else {
		input_report_key(strip->input, BTN_TOUCH, 0);
	}
	input_sync(strip->input);

	return IRQ_HANDLED;
}

static int l16_strip_probe(struct i2c_client *client)
{
	struct device *dev = &client->dev;
	struct l16_strip *strip;
	int ret;

	strip = devm_kzalloc(dev, sizeof(*strip), GFP_KERNEL);
	if (!strip)
		return -ENOMEM;
	strip->client = client;

	ret = devm_regulator_get_enable(dev, "vcc33");
	if (ret)
		return dev_err_probe(dev, ret, "vcc33 supply\n");
	ret = devm_regulator_get_enable(dev, "vccio");
	if (ret)
		return dev_err_probe(dev, ret, "vccio supply\n");

	/* reset: held low 10 ms, then released; the controller says hello after ~0.5 s */
	strip->reset_gpio = devm_gpiod_get_optional(dev, "reset", GPIOD_OUT_HIGH);
	if (IS_ERR(strip->reset_gpio))
		return dev_err_probe(dev, PTR_ERR(strip->reset_gpio), "reset-gpios\n");
	if (strip->reset_gpio) {
		msleep(10);
		gpiod_set_value_cansleep(strip->reset_gpio, 0);
	}

	strip->input = devm_input_allocate_device(dev);
	if (!strip->input)
		return -ENOMEM;
	strip->input->name = "Light L16 touch strip";
	strip->input->id.bustype = BUS_I2C;
	input_set_capability(strip->input, EV_KEY, BTN_TOUCH);
	input_set_abs_params(strip->input, ABS_X, 0, STRIP_X_MAX, 0, 0);

	ret = input_register_device(strip->input);
	if (ret)
		return ret;

	/* level low; the handler always reads a packet, which releases the line */
	return devm_request_threaded_irq(dev, client->irq, NULL, l16_strip_irq,
					 IRQF_ONESHOT, "l16-touchstrip", strip);
}

static const struct of_device_id l16_strip_of_match[] = {
	{ .compatible = "light,l16-touchstrip" },
	{ }
};
MODULE_DEVICE_TABLE(of, l16_strip_of_match);

static struct i2c_driver l16_strip_driver = {
	.driver = {
		.name = "l16-touchstrip",
		.of_match_table = l16_strip_of_match,
	},
	.probe = l16_strip_probe,
};
module_i2c_driver(l16_strip_driver);

MODULE_DESCRIPTION("Light L16 touch strip (Elan eKTF, one-dimensional)");
MODULE_LICENSE("GPL");
