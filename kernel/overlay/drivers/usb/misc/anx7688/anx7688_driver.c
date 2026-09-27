/*
 * Analogix ANX7688 (OHIO) USB Type-C / PD controller - Light L16 driver.
 *
 * Reconstructed from the stock L16 kernel (LFC 1.3.5.1). The original is
 * Analogix's reference driver, version 2.1.11, as integrated by FIH.
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License version 2 and
 * only version 2 as published by the Free Software Foundation.
 */

#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/init.h>
#include <linux/slab.h>
#include <linux/delay.h>
#include <linux/gpio/consumer.h>
#include <linux/of.h>
#include <linux/i2c.h>
#include <linux/power_supply.h>
#include <linux/regulator/consumer.h>
#include <linux/interrupt.h>
#include <linux/async.h>
#include <linux/string.h>
#include "anx7688.h"

unchar device_addr = OHIO_SLAVE_I2C_ADDR;
unchar debug_on;
unchar ocm_bootload_done;
int anx7688_power_status;
struct i2c_client *anx7688_client;
static struct anx7688_platform_data *g_pdata;

/*
 * The stock driver checks the ANX7688 OCM firmware against the copy built
 * into the kernel on every boot and reflashes the chip if they differ (or
 * if the OCM is slow to report ready). That path is only built with
 * CONFIG_ANX7688_FW_UPDATE; otherwise the chip's firmware is never touched.
 */
unchar auto_update;

inline unchar ReadReg(unchar RegAddr)
{
	int ret;

	anx7688_client->addr = (device_addr >> 1);
	ret = i2c_smbus_read_byte_data(anx7688_client, RegAddr);
	if (ret < 0)
		pr_err("%s %s: failed to read i2c addr=%x\n", LOG_TAG, "", device_addr);

	return (unchar)ret;
}

inline int ReadBlockReg(unchar RegAddr, unchar len, unchar *dat)
{
	int ret;

	anx7688_client->addr = (device_addr >> 1);
	ret = i2c_smbus_read_i2c_block_data(anx7688_client, RegAddr, len, dat);
	if (ret < 0) {
		pr_err("%s %s: failed to read i2c block addr=%x\n", LOG_TAG, "", device_addr);
		return -EPERM;
	}

	return ret;
}

inline int WriteBlockReg(unchar RegAddr, unchar len, const unchar *dat)
{
	int ret;

	anx7688_client->addr = (device_addr >> 1);
	ret = i2c_smbus_write_i2c_block_data(anx7688_client, RegAddr, len, dat);
	if (ret < 0) {
		pr_err("%s %s: failed to read i2c block addr=%x\n", LOG_TAG, "", device_addr);
		return -EPERM;
	}

	return ret;
}

inline void WriteReg(unchar RegAddr, unchar RegVal)
{
	int ret;

	anx7688_client->addr = (device_addr >> 1);
	ret = i2c_smbus_write_byte_data(anx7688_client, RegAddr, RegVal);
	if (ret < 0)
		pr_err("%s %s: failed to write i2c addr=%x\n", LOG_TAG, "", device_addr);
}

int get_disport_capability(unchar *dp_pin_assign, unchar *dp_signaling)
{
	unchar pin, sig;

	if (g_pdata->cbl_det_status && (ReadReg(OHIO_OCM_LOAD_STATUS) & 0x01)) {
		device_addr = OHIO_OCM_I2C_ADDR;
		pin = ReadReg(OHIO_DP_PIN_ASSIGN);
		sig = ReadReg(OHIO_DP_SIGNALING);
		device_addr = OHIO_SLAVE_I2C_ADDR;

		switch (pin) {
		case 0x06:
			pin = 0;
			break;
		case 0x0a:
			pin = 1;
			break;
		case 0x14:
			pin = 2;
			break;
		case 0x19:
			pin = 3;
			break;
		default:
			goto unknown;
		}

		switch (sig) {
		case 0x01:
		case 0x02:
			break;
		case 0x04:
			sig = 2;
			break;
		default:
			goto unknown;
		}

		*dp_pin_assign = pin;
		*dp_signaling = sig;
		return 0;
	}

unknown:
	*dp_pin_assign = 3;
	*dp_signaling = 2;
	return -1;
}
EXPORT_SYMBOL(get_disport_capability);

/*
 * Stock passed the Type-C / PD current capability to FIH's smbcharger, which used
 * it on SDP-detected ports. Mainline equivalent: raise the qcom-smbchg input limit
 * on an SDP port to what the port advertises, capped at stock's USB input maximum
 * (qcom,usb-psy-ma = 2000 mA). DCP/CDP inputs are left to the charger's AICL.
 */
#define ANX7688_ILIM_MAX_MA	2000

void anx7688_set_current_capability(struct anx7688_data *platform, int current_ma)
{
	union power_supply_propval val;
	int ret;

	platform->current_capability = current_ma;
	if (current_ma <= 500)
		return;

	if (!platform->chg_psy)
		platform->chg_psy = power_supply_get_by_name("qcom-smbchg-usb");
	if (!platform->chg_psy)
		return;

	ret = power_supply_get_property(platform->chg_psy, POWER_SUPPLY_PROP_USB_TYPE, &val);
	if (ret || val.intval != POWER_SUPPLY_USB_TYPE_SDP)
		return;

	val.intval = min(current_ma, ANX7688_ILIM_MAX_MA) * 1000;
	ret = power_supply_set_property(platform->chg_psy,
					POWER_SUPPLY_PROP_INPUT_CURRENT_LIMIT, &val);
	pr_info("%s: SDP input limit -> %d uA (port offers %d mA): %d\n",
		LOG_TAG, val.intval, current_ma, ret);
}

static unchar confirmed_cable_det(void *data)
{
	struct anx7688_data *anxdata = data;
	unsigned int count = 10;
	unsigned int cable_det_count = 0;
	u8 val;

	do {
		val = gpiod_get_value(anxdata->pdata->gpio_cbl_det);
		if (val == 1)
			cable_det_count++;
		mdelay(1);
	} while (--count);

	if (cable_det_count > 7)
		return 1;
	else if (cable_det_count < 3)
		return 0;
	else
		return anx7688_power_status;
}

ssize_t anx7688_select_rdo_index(struct device *dev, struct device_attribute *attr,
				 const char *buf, size_t count)
{
	int cmd;

	cmd = sscanf(buf, "%d", &cmd);
	if (cmd <= 0)
		return 0;

	pr_info("NewRDO idx %d, Old idx %d\n", cmd, sel_voltage_pdo_index);
	sel_voltage_pdo_index = cmd;
	return count;
}

ssize_t anx7688_chg_addr(struct device *dev, struct device_attribute *attr,
			 const char *buf, size_t count)
{
	int addr;

	sscanf(buf, "%x", &addr);
	device_addr = addr;
	pr_info("Change Device Address to 0x%x\n", device_addr);
	return count;
}

ssize_t anx7688_rd_reg(struct device *dev, struct device_attribute *attr,
		       const char *buf, size_t count)
{
	int cmd;

	sscanf(buf, "%x", &cmd);
	printk("reg[%x] = %x\n", cmd, ReadReg(cmd));
	return count;
}

ssize_t anx7688_wr_reg(struct device *dev, struct device_attribute *attr,
		       const char *buf, size_t count)
{
	int cmd, val;

	sscanf(buf, "%x  %x", &cmd, &val);
	pr_info("c %x val %x\n", cmd, val);
	WriteReg(cmd, val);
	pr_info("reg[%x] = %x\n", cmd, ReadReg(cmd));
	return count;
}

ssize_t anx7688_send_pswap(struct device *dev, struct device_attribute *attr, char *buf)
{
	return snprintf(buf, 1, "%d\n", send_power_swap());
}

ssize_t anx7688_get_data_role(struct device *dev, struct device_attribute *attr, char *buf)
{
	return snprintf(buf, 1, "%d\n", get_data_role());
}

ssize_t anx7688_get_power_role(struct device *dev, struct device_attribute *attr, char *buf)
{
	return snprintf(buf, 1, "%d\n", get_power_role());
}

ssize_t anx7688_dump_register(struct device *dev, struct device_attribute *attr, char *buf)
{
	int i;

	for (i = 0; i < 256; i++) {
		if (i % 0x10 == 0)
			pr_info("\n");
		printk(" %.2x", ReadReg(i));
		snprintf(&buf[i], 1, "%d", ReadReg(i));
	}
	printk("\n");

	return i;
}

ssize_t anx7688_chip_version(struct device *dev, struct device_attribute *attr, char *buf)
{
	unchar id_h = ReadReg(OHIO_CHIP_ID_H);
	unchar id_l = ReadReg(OHIO_CHIP_ID_L);
	unchar rev = ReadReg(OHIO_CHIP_REV);

	return snprintf(buf, 8, "%2x%2x%2x\n", id_h, id_l, rev);
}

ssize_t anx7688_fw_version(struct device *dev, struct device_attribute *attr, char *buf)
{
	unchar major = ReadReg(OHIO_OCM_FW_VER_MAJOR);
	unchar minor = ReadReg(OHIO_OCM_FW_VER_MINOR);

	return snprintf(buf, 8, "%2x.%2x\n", major, minor);
}

ssize_t anx7688_send_dswap(struct device *dev, struct device_attribute *attr, char *buf)
{
	return snprintf(buf, 1, "%d\n", send_data_swap());
}

ssize_t anx7688_try_source(struct device *dev, struct device_attribute *attr, char *buf)
{
	return snprintf(buf, 1, "%d\n", try_source());
}

ssize_t anx7688_try_sink(struct device *dev, struct device_attribute *attr, char *buf)
{
	return snprintf(buf, 1, "%d\n", try_sink());
}


ssize_t anx7688_typec_position(struct device *dev, struct device_attribute *attr, char *buf)
{
	unchar val;

	if (g_pdata->cbl_det_status && (ReadReg(OHIO_OCM_LOAD_STATUS) & 0x01)) {
		val = ReadReg(OHIO_ANALOG_STATUS);
		if (val & 0xe0)
			return snprintf(buf, 2, "1\n");
		if (val & 0x1c)
			return snprintf(buf, 2, "0\n");
	}

	return snprintf(buf, 3, "-1\n");
}

static irqreturn_t anx7688_intr_comm_isr(int irq, void *data)
{
	struct anx7688_data *platform = data;
	unchar c;

	if (anx7688_power_status != 1 || ocm_bootload_done != 1)
		return IRQ_NONE;

	device_addr = OHIO_OCM_I2C_ADDR;
	c = ReadReg(OHIO_OCM_IRQ);
	if (c)
		WriteReg(OHIO_OCM_IRQ, 0);
	device_addr = OHIO_SLAVE_I2C_ADDR;

	c = ReadReg(OHIO_IRQ_SOURCE);
	if (c & 0x04) {
		pr_info("%s %s : ======I=====\n", LOG_TAG, "");
		handle_intr_vector(platform);
	}

	return IRQ_HANDLED;
}

static void anx7688_cc_detect_result(void)
{
	unchar cc_status = ReadReg(OHIO_CC_STATUS) & 0x0f;

	if (cc_status == 0x05) {
		pr_info("%s %s: Debug accessory mode\n", LOG_TAG, "");
		WriteReg(0x42, 0x61);
		WriteReg(0x46, 0x80);
	} else {
		pr_info("%s %s: CC status=0x%2x\n", LOG_TAG, "", cc_status);
	}
}

static inline void anx7688_vconn_5v_regulator(bool on)
{
	int rc;

	pr_err("%s: anx7688_vconn_5v_regulator: enter...\n", LOG_TAG);

	if (!g_pdata->vconn_5v) {
		pr_err("%s: pdata->vconn_5v failed\n", LOG_TAG);
		return;
	}

	if (on) {
		rc = regulator_enable(g_pdata->vconn_5v);
		if (rc)
			pr_err("%s: Failed to enable vconn_5v\n", LOG_TAG);
	} else {
		rc = regulator_disable(g_pdata->vconn_5v);
		if (rc)
			pr_err("%s: Failed to disable vconn_5v\n", LOG_TAG);
	}
}

static void anx7688_i2c_remove(struct i2c_client *client)
{
	struct anx7688_data *platform = i2c_get_clientdata(client);

	free_irq(client->irq, platform);
	free_irq(gpiod_to_irq(platform->pdata->gpio_cbl_det), platform);
	cancel_delayed_work_sync(&platform->work);
	destroy_workqueue(platform->workqueue);
	anx7688_power_standby();
	if (platform->chg_psy)
		power_supply_put(platform->chg_psy);
	kfree(platform);
}

void MI1_power_on(void)
{
	struct anx7688_platform_data *pdata = g_pdata;

	gpiod_set_value(pdata->gpio_p_on, 1);
	mdelay(10);
	gpiod_set_value(pdata->gpio_reset, 1);
	mdelay(10);
	pr_info("%s %s: MI-1 power on !\n", LOG_TAG, "");
}

void anx7688_hardware_reset(int enable)
{
	gpiod_set_value(g_pdata->gpio_reset, enable);
}

void anx7688_power_standby(void)
{
	struct anx7688_platform_data *pdata = g_pdata;

	gpiod_set_value(pdata->gpio_reset, 0);
	mdelay(1);
	gpiod_set_value(pdata->gpio_p_on, 0);
	mdelay(1);
	pr_info("%s %s: anx7688 power down\n", LOG_TAG, "");
}

ssize_t anx7688_send_pd_cmd(struct device *dev, struct device_attribute *attr,
			    const char *buf, size_t count);
ssize_t anx7688_debug(struct device *dev, struct device_attribute *attr,
		      const char *buf, size_t count);

static struct device_attribute anx7688_device_attrs[] = {
	__ATTR(pdcmd, S_IWUSR, NULL, anx7688_send_pd_cmd),
	__ATTR(rdreg, S_IWUSR, NULL, anx7688_rd_reg),
	__ATTR(wrreg, S_IWUSR, NULL, anx7688_wr_reg),
	__ATTR(addr, S_IWUSR, NULL, anx7688_chg_addr),
	__ATTR(rdoidx, S_IWUSR, NULL, anx7688_select_rdo_index),
	__ATTR(dumpreg, S_IRUGO, anx7688_dump_register, NULL),
	__ATTR(prole, S_IRUGO, anx7688_get_power_role, NULL),
	__ATTR(drole, S_IRUGO, anx7688_get_data_role, NULL),
	__ATTR(trysrc, S_IRUGO, anx7688_try_source, NULL),
	__ATTR(trysink, S_IRUGO, anx7688_try_sink, NULL),
	__ATTR(pswap, S_IRUGO, anx7688_send_pswap, NULL),
	__ATTR(dswap, S_IRUGO, anx7688_send_dswap, NULL),
	__ATTR(cmd, S_IWUSR, NULL, anx7688_debug),
	__ATTR(chip_version, S_IRUGO, anx7688_chip_version, NULL),
	__ATTR(fw_version, S_IRUGO, anx7688_fw_version, NULL),
	__ATTR(position, S_IRUGO, anx7688_typec_position, NULL),
};

static inline int create_sysfs_interfaces(struct device *dev)
{
	int i;

	pr_info("anx7688 create system fs interface ...\n");
	for (i = 0; i < ARRAY_SIZE(anx7688_device_attrs); i++)
		if (device_create_file(dev, &anx7688_device_attrs[i]))
			goto error;
	pr_info("success\n");
	return 0;

error:
	for (; i >= 0; i--)
		device_remove_file(dev, &anx7688_device_attrs[i]);
	pr_err("%s %s: anx7688 Unable to create interface", LOG_TAG, "");
	return -EINVAL;
}

/*
 * Same pins as stock: cable detect and interrupt as inputs, DP LDO enabled.
 *
 * Stock requests power-on and reset as outputs driven low (chip off). That runs early
 * in the stock boot, before USB is up; on mainline the chip is usually still powered
 * from the previous boot with a USB session live, and switching it off here drops the
 * Type-C connection under the host. So keep their current level; the probe powers the
 * chip down only when no cable is attached, and the work function then runs the stock
 * power-on / OCM-ready / init sequence.
 */
static struct gpio_desc *anx7688_get_output_keep(struct device *dev, const char *con_id)
{
	struct gpio_desc *desc = devm_gpiod_get(dev, con_id, GPIOD_ASIS);
	int ret;

	if (IS_ERR(desc))
		return desc;
	ret = gpiod_direction_output(desc, gpiod_get_value(desc));
	return ret ? ERR_PTR(ret) : desc;
}

static int anx7688_init_gpio(struct device *dev, struct anx7688_platform_data *pdata)
{
	pdata->gpio_p_on = anx7688_get_output_keep(dev, "analogix,p-on");
	if (IS_ERR(pdata->gpio_p_on))
		return dev_err_probe(dev, PTR_ERR(pdata->gpio_p_on), "p-on gpio\n");
	pdata->gpio_reset = anx7688_get_output_keep(dev, "analogix,reset");
	if (IS_ERR(pdata->gpio_reset))
		return dev_err_probe(dev, PTR_ERR(pdata->gpio_reset), "reset gpio\n");
	pdata->gpio_cbl_det = devm_gpiod_get(dev, "analogix,cbl-det", GPIOD_IN);
	if (IS_ERR(pdata->gpio_cbl_det))
		return dev_err_probe(dev, PTR_ERR(pdata->gpio_cbl_det), "cbl-det gpio\n");
	pdata->gpio_intr_comm = devm_gpiod_get(dev, "analogix,intr-comm", GPIOD_IN);
	if (IS_ERR(pdata->gpio_intr_comm))
		return dev_err_probe(dev, PTR_ERR(pdata->gpio_intr_comm), "intr-comm gpio\n");
	pdata->gpio_dp_ldo_on = devm_gpiod_get(dev, "analogix,dp-ldo-on", GPIOD_OUT_HIGH);
	if (IS_ERR(pdata->gpio_dp_ldo_on))
		return dev_err_probe(dev, PTR_ERR(pdata->gpio_dp_ldo_on), "dp-ldo-on gpio\n");

	pr_info("%s: gpios: p_on=%d reset=%d cbl_det=%d\n", LOG_TAG,
		gpiod_get_value(pdata->gpio_p_on), gpiod_get_value(pdata->gpio_reset),
		gpiod_get_value(pdata->gpio_cbl_det));
	return 0;
}

static irqreturn_t anx7688_cbl_det_isr(int irq, void *data);
static void anx7688_work_func(struct work_struct *work);

static int anx7688_i2c_probe(struct i2c_client *client)
{
	struct anx7688_data *platform;
	struct anx7688_platform_data *pdata;
	int ret = 0;
	int cbl_det_irq = 0;

	if (!i2c_check_functionality(client->adapter, I2C_FUNC_SMBUS_I2C_BLOCK)) {
		pr_err("%s:anx7688's i2c bus doesn't support\n", "");
		return -ENODEV;
	}

	platform = kzalloc(sizeof(struct anx7688_data), GFP_KERNEL);
	if (!platform)
		return -ENOMEM;

	pdata = devm_kzalloc(&client->dev, sizeof(struct anx7688_platform_data), GFP_KERNEL);
	if (!pdata) {
		ret = -ENOMEM;
		goto exit;
	}
	client->dev.platform_data = pdata;
	platform->pdata = pdata;

	g_pdata = platform->pdata;
	anx7688_client = client;
	anx7688_client->addr = (device_addr >> 1);
	anx7688_power_status = 0;
	pdata->cbl_det_status = 0;
	mutex_init(&platform->lock);

	/* VCONN supply (pmi8994 5 V boost) is optional; only e-marked cables/accessories need it */
	pdata->vconn_5v = devm_regulator_get_optional(&client->dev, "analogix,vconn_5v");
	if (IS_ERR(pdata->vconn_5v))
		pdata->vconn_5v = NULL;
	else
		anx7688_vconn_5v_regulator(1);

	ret = anx7688_init_gpio(&client->dev, pdata);
	if (ret)
		goto exit;

	INIT_DELAYED_WORK(&platform->work, anx7688_work_func);
	platform->client = client;
	i2c_set_clientdata(client, platform);
	platform->current_capability = 0;

	platform->workqueue = create_singlethread_workqueue("anx7688_work");
	if (!platform->workqueue) {
		ret = -ENOMEM;
		goto exit;
	}

	cbl_det_irq = gpiod_to_irq(pdata->gpio_cbl_det);
	if (cbl_det_irq < 0) {
		ret = cbl_det_irq;
		goto err1;
	}

	ret = request_threaded_irq(cbl_det_irq, NULL, anx7688_cbl_det_isr,
				   IRQF_TRIGGER_RISING | IRQF_TRIGGER_FALLING | IRQF_ONESHOT,
				   "anx7688-cbl-det", platform);
	if (ret < 0) {
		pr_err("%s : failed to request irq\n", "");
		goto err1;
	}
	enable_irq_wake(cbl_det_irq);

	client->irq = gpiod_to_irq(pdata->gpio_intr_comm);
	if (client->irq < 0) {
		ret = client->irq;
		goto err2;
	}

	ret = request_threaded_irq(client->irq, NULL, anx7688_intr_comm_isr,
				   IRQF_TRIGGER_FALLING | IRQF_ONESHOT,
				   "anx7688-intr-comm", platform);
	if (ret < 0) {
		pr_err("%s : failed to request interface irq\n", "");
		goto err2;
	}
	enable_irq_wake(client->irq);

	ret = create_sysfs_interfaces(&client->dev);
	if (ret < 0) {
		pr_err("%s : sysfs register failed", "");
		goto err3;
	}

	/* delay 1000ms to check cable status */
	queue_delayed_work(platform->workqueue, &platform->work, msecs_to_jiffies(1000));
	/* stock powers down unconditionally here; keep a chip with a cable attached powered */
	if (!confirmed_cable_det(platform))
		anx7688_power_standby();

	pr_info("anx7688_i2c_probe successfully %s %s end\n", LOG_TAG, "");
	return 0;

err3:
	free_irq(client->irq, platform);
err2:
	free_irq(cbl_det_irq, platform);
err1:
	destroy_workqueue(platform->workqueue);
exit:
	anx7688_client = NULL;
	kfree(platform);
	return ret;
}

ssize_t anx7688_debug(struct device *dev, struct device_attribute *attr,
		      const char *buf, size_t count)
{
	int param[4] = { 0 };
	char CommandName[16];
	int ret, i;

	ret = sscanf(buf, "%s %d %d %d %d", CommandName, &param[0], &param[1], &param[2], &param[3]);
	printk("anx7688 cmd[%s", CommandName);
	for (i = 0; i < ret - 1; i++)
		printk(" %d", param[i]);
	printk("]\n");

	if (strcmp(CommandName, "poweron") == 0) {
		printk("MI1_power_on\n");
		MI1_power_on();
	} else if (strcmp(CommandName, "powerdown") == 0) {
		anx7688_power_standby();
	} else if (strcmp(CommandName, "debugon") == 0) {
		debug_on = 1;
		printk("debug_on = %d\n", debug_on);
	} else if (strcmp(CommandName, "debugoff") == 0) {
		debug_on = 0;
		printk("debug_on = %d\n", debug_on);
	} else {
		printk("Usage:\n");
		printk("  echo poweron > cmd       : power on\n");
		printk("  echo powerdown > cmd     : power off\n");
		printk("  echo debugon > cmd       : debug on\n");
		printk("  echo debugoff > cmd      : debug off\n");
	}

	return count;
}

void anx7688_hardware_poweron(void)
{
	struct anx7688_platform_data *pdata = g_pdata;
	int retry_count, i;

	pr_info("%s %s: anx7688 power on\n", LOG_TAG, "");

	for (retry_count = 0; retry_count < 3; retry_count++) {
		pr_info("%s %s: anx7688 check ocm loading...\n", LOG_TAG, "");

		/* power on pin enable */
		gpiod_set_value(pdata->gpio_p_on, 1);
		mdelay(10);

		/* power reset pin enable */
		gpiod_set_value(pdata->gpio_reset, 1);
		mdelay(10);

		/* wait for the OCM to finish loading its firmware */
		for (i = 0; i < 3200; i++) {
			if (ReadReg(OHIO_OCM_LOAD_STATUS) & 0x01) {
				unchar major, minor;

				pr_info("%s %s: interface initialization\n", LOG_TAG, "");
				chip_register_init();
				interface_init();
				if (!pdata->cbl_det_status)
					pr_err("%s %s: cable disconnected\n", LOG_TAG, "");
				else
					send_initialized_setting();

				major = ReadReg(OHIO_OCM_FW_VER_MAJOR);
				minor = ReadReg(OHIO_OCM_FW_VER_MINOR);
				pr_info("%s %s: chip is power on! firmware version is %02x%02x, Driver version: %s\n",
					LOG_TAG, "", major, minor, ANX_DRV_VERSION);
				ocm_bootload_done = 1;
				return;
			}
			mdelay(1);
		}

		anx7688_power_standby();
		mdelay(10);
	}
}

static irqreturn_t anx7688_cbl_det_isr(int irq, void *data)
{
	struct anx7688_data *platform = data;
	struct anx7688_platform_data *pdata = platform->pdata;

	if (debug_on)
		return IRQ_NONE;

	pdata->cbl_det_status = confirmed_cable_det(platform);
	pr_info("%s %s : cable plug pin status %d\n", LOG_TAG, "", pdata->cbl_det_status);

	if (pdata->cbl_det_status == 1) {
		if (anx7688_power_status == 1) {
			mdelay(2);
		} else {
			ocm_bootload_done = 0;
			anx7688_power_status = 1;
			anx7688_hardware_poweron();
			anx7688_cc_detect_result();
		}
	} else {
		anx7688_power_status = 0;
		anx7688_power_standby();
		platform->current_capability = 0;
	}

	return IRQ_HANDLED;
}

void anx7688_vbus_control(bool value)
{
}

void anx7688_main_process(void)
{
}

static void anx7688_work_func(struct work_struct *work)
{
	struct anx7688_data *td = container_of(work, struct anx7688_data, work.work);

	mutex_lock(&td->lock);
	anx7688_main_process();

	td->pdata->cbl_det_status = confirmed_cable_det(td);
	pr_info("%s %s : cable status: %d\n", LOG_TAG, "", td->pdata->cbl_det_status);

	anx7688_power_status = td->pdata->cbl_det_status;
	if (anx7688_power_status == 1) {
		anx7688_hardware_poweron();
		anx7688_cc_detect_result();
	} else {
		anx7688_power_status = 0;
	}
	mutex_unlock(&td->lock);
}

void dump_reg(void)
{
	int i;
	unchar val;

	printk("dump registerad:\n");
	printk("     0  1  2  3  4  5  6  7  8  9  A  B  C  D  E  F\n");
	for (i = 0; i < 256; i++) {
		val = ReadReg(i);
		if ((i & 0x0f) == 0)
			printk("\n[%x]:%02x ", i, val);
		else
			printk("%02x ", val);
	}
	printk("\n");
}

ssize_t anx7688_send_pd_cmd(struct device *dev, struct device_attribute *attr,
			    const char *buf, size_t count)
{
	int cmd;

	sscanf(buf, "%d", &cmd);

	switch (cmd) {
	case TYPE_PWR_SRC_CAP:
		send_pd_msg(TYPE_PWR_SRC_CAP, 0, 0);
		break;
	case TYPE_DP_SNK_IDENTITY:
		send_pd_msg(TYPE_DP_SNK_IDENTITY, 0, 0);
		break;
	case TYPE_PSWAP_REQ:
		send_pd_msg(TYPE_PSWAP_REQ, 0, 0);
		break;
	case TYPE_DSWAP_REQ:
		send_pd_msg(TYPE_DSWAP_REQ, 0, 0);
		break;
	case TYPE_GOTO_MIN_REQ:
		send_pd_msg(TYPE_GOTO_MIN_REQ, 0, 0);
		break;
	case TYPE_PWR_OBJ_REQ:
		interface_send_request();
		break;
	case TYPE_ACCEPT:
		interface_send_accept();
		break;
	case TYPE_REJECT:
		interface_send_reject();
		break;
	case TYPE_SOFT_RST:
		send_pd_msg(TYPE_SOFT_RST, 0, 0);
		break;
	case TYPE_HARD_RST:
		send_pd_msg(TYPE_HARD_RST, 0, 0);
		break;
	case 0xfd:
		pr_info("fetch powerrole: %d\n", get_power_role());
		break;
	case 0xfe:
		pr_info("fetch datarole: %d\n", get_data_role());
		break;
	case 0xff:
		dump_reg();
		break;
	default:
		break;
	}

	return count;
}

static const struct i2c_device_id anx7688_id[] = {
	{ "anx7688", 0 },
	{ }
};
MODULE_DEVICE_TABLE(i2c, anx7688_id);

static struct of_device_id anx_match_table[] = {
	{ .compatible = "analogix,anx7688", },
	{ },
};

static struct i2c_driver anx7688_driver = {
	.driver = {
		.name = "anx7688",
		.owner = THIS_MODULE,
		.of_match_table = anx_match_table,
	},
	.probe = anx7688_i2c_probe,
	.remove = anx7688_i2c_remove,
	.id_table = anx7688_id,
};

static void __init anx7688_init_async(void *data, async_cookie_t cookie)
{
	int ret;

	ret = i2c_add_driver(&anx7688_driver);
	if (ret < 0)
		pr_err("%s: failed to register anx7688 i2c drivern", "");
}

static int __init anx7688_init(void)
{
	async_schedule(anx7688_init_async, NULL);
	return 0;
}

static void __exit anx7688_exit(void)
{
	i2c_del_driver(&anx7688_driver);
}

module_init(anx7688_init);
module_exit(anx7688_exit);

MODULE_DESCRIPTION("USB PD Anx7688 driver");
MODULE_LICENSE("GPL v2");
MODULE_VERSION(ANX_DRV_VERSION);
