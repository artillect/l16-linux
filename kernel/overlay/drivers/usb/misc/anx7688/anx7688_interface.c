/*
 * Analogix ANX7688 (OHIO) USB Type-C / PD controller - PD message interface.
 *
 * Reconstructed from the stock L16 kernel (LFC 1.3.5.1). The original is
 * Analogix's reference driver, version 2.1.11, as integrated by FIH.
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License version 2 and
 * only version 2 as published by the Free Software Foundation.
 */

#include <linux/kernel.h>
#include <linux/delay.h>
#include <linux/jiffies.h>
#include <linux/string.h>
#include "anx7688.h"

u8 pbuf_rx_front;
u8 pbuf_tx_rear;
u8 downstream_pd_cap;
u8 pd_snk_pdo[32];
u8 configure_DP_caps[8];
u8 src_dp_caps[8];
u8 pd_rdo[4];
static pd_callback_t pd_callback_array[256];

/* Initial PD capabilities sent once the OCM is up (little-endian PDOs). */
static u8 init_src_caps[8] = {
	0x96, 0x90, 0x01, 0x22,		/* 5V 1.5A fixed */
};
static u8 init_snk_cap[16] = {
	0x5a, 0x90, 0x01, 0x22,		/* 5V 0.9A fixed */
	0x3c, 0x90, 0x41, 0x5a,		/* 5-5V 0.6A variable */
	0x2c, 0x91, 0x41, 0x9a,		/* 5-5V 3W battery */
};
static u8 init_snk_ident[16] = {
	0x00, 0x00, 0x00, 0xec,
	0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00,
	0x39, 0x00, 0x00, 0x51,
};
static u8 init_svid[4] = { 0x00, 0x00, 0x01, 0xff };

u8 pd_src_pdo[28] = {
	0x5a, 0x90, 0x01, 0x2a,
	0x96, 0x90, 0x01, 0x2a,
};
u8 pd_src_pdo_cnt = 2;
u8 pd_snk_pdo_cnt = 3;
u8 sel_voltage_pdo_index = 2;

u8 get_otp_indicator_byte(void)
{
	u8 c;

	WriteReg(0xe5, 0xa0);
	WriteReg(0xef, 0x7a);
	WriteReg(0xd0, 0x00);
	WriteReg(0xd1, 0x01);
	WriteReg(0xe5, 0xa1);
	while (ReadReg(0xed) & 0x30)
		;
	c = ReadReg(0xe0);
	WriteReg(0xef, 0x00);
	WriteReg(0xe5, 0x00);

	return c;
}

u8 get_data_role(void)
{
	return (ReadReg(OHIO_SYSTEM_STATUS) >> 5) & 0x01;
}

u8 get_power_role(void)
{
	return !((ReadReg(OHIO_POWER_CTRL) >> 3) & 0x01);
}

u8 get_src_cap(const u8 *src_caps, u8 src_caps_size)
{
	return 1;
}

u8 get_snk_cap(const u8 *snk_caps, u8 snk_caps_size)
{
	return 1;
}

char *interface_to_str(unsigned char header_type)
{
	return (header_type == TYPE_PWR_SRC_CAP) ? "src cap" :
	       (header_type == TYPE_PWR_SNK_CAP) ? "snk cap" :
	       (header_type == TYPE_PWR_OBJ_REQ) ? "RDO" :
	       (header_type == TYPE_DP_SNK_IDENTITY) ? "snk identity" :
	       (header_type == TYPE_SVID) ? "svid" :
	       (header_type == TYPE_PSWAP_REQ) ? "PR_SWAP" :
	       (header_type == TYPE_DSWAP_REQ) ? "DR_SWAP" :
	       (header_type == TYPE_GOTO_MIN_REQ) ? "GOTO_MIN" :
	       (header_type == TYPE_DP_ALT_ENTER) ? "DPALT_ENTER" :
	       (header_type == TYPE_DP_ALT_EXIT) ? "DPALT_EXIT" :
	       (header_type == TYPE_VCONN_SWAP_REQ) ? "VCONN_SWAP" :
	       (header_type == TYPE_GET_DP_SNK_CAP) ? "GET_SINK_DP_CAP" :
	       (header_type == TYPE_DP_SNK_CFG) ? "dp cap" :
	       (header_type == TYPE_SOFT_RST) ? "Soft Reset" :
	       (header_type == TYPE_HARD_RST) ? "Hard Reset" :
	       (header_type == TYPE_RESTART) ? "Restart" :
	       (header_type == TYPE_PD_STATUS_REQ) ? "PD Status" :
	       (header_type == TYPE_ACCEPT) ? "ACCEPT" :
	       (header_type == TYPE_REJECT) ? "REJECT" :
	       (header_type == TYPE_VDM) ? "VDM" :
	       (header_type == TYPE_RESPONSE_TO_REQ) ? "Response to Request" :
	       "Unknown";
}

inline u8 cac_checksum(u8 *pSendBuf, u8 len)
{
	u8 i;
	u8 checksum = 0;

	for (i = 0; i < len; i++)
		checksum += *(pSendBuf + i);

	return (u8)(0 - checksum);
}

void printb(const char *buf, size_t size)
{
	while (size--)
		printk("%0x ", *buf++);
	printk("\n");
}

void interface_init(void)
{
	pbuf_rx_front = 0;
	pbuf_tx_rear = 0;
	downstream_pd_cap = 0;
}

void send_initialized_setting(void)
{
	unsigned char send_init_setting_state, c;

	send_init_setting_state = 1;

	for (;;) {
		switch (send_init_setting_state) {
		case 1:
			/* send TYPE_PWR_SRC_CAP init setting */
			send_pd_msg(TYPE_PWR_SRC_CAP, init_src_caps, 4);
			send_init_setting_state++;
			break;
		case 2:
			device_addr = OHIO_OCM_I2C_ADDR;
			c = ReadReg(OHIO_INTERFACE_TX_BUF);
			if (c != 0)
				break;
			send_init_setting_state++;
			/* fallthrough */
		case 3:
			/* send TYPE_PWR_SNK_CAP init setting */
			send_pd_msg(TYPE_PWR_SNK_CAP, init_snk_cap, 12);
			send_init_setting_state++;
			break;
		case 4:
			device_addr = OHIO_OCM_I2C_ADDR;
			c = ReadReg(OHIO_INTERFACE_TX_BUF);
			if (c != 0)
				break;
			send_init_setting_state++;
			/* fallthrough */
		case 5:
			/* send TYPE_DP_SNK_IDENTITY init setting */
			send_pd_msg(TYPE_DP_SNK_IDENTITY, init_snk_ident, sizeof(init_snk_ident));
			send_init_setting_state++;
			break;
		case 6:
			device_addr = OHIO_OCM_I2C_ADDR;
			c = ReadReg(OHIO_INTERFACE_TX_BUF);
			if (c != 0)
				break;
			send_init_setting_state++;
			/* fallthrough */
		case 7:
			/* send TYPE_SVID init setting */
			send_pd_msg(TYPE_SVID, init_svid, sizeof(init_svid));
			send_init_setting_state++;
			break;
		case 8:
		case 9:
			return;
		default:
			break;
		}
	}
}

void chip_register_init(void)
{
	/* set the minimum PD power parameters */
	WriteReg(OHIO_FW_CTRL_2, 0x96);
	WriteReg(OHIO_FW_CTRL, 0x19);
	WriteReg(0x17, 0xa6);
	WriteReg(OHIO_FW_CTRL_3, ReadReg(OHIO_FW_CTRL_3) | 0x02);

	WriteReg(OHIO_MAX_VOLTAGE, 0x32);	/* 5V (100mV units) */
	WriteReg(OHIO_MAX_POWER, 0x1e);		/* 15W (500mW units) */
	WriteReg(OHIO_MIN_POWER, 0x14);		/* 10W (500mW units) */
	WriteReg(OHIO_FW_CTRL_3, ReadReg(OHIO_FW_CTRL_3) | 0x08);
}

inline void reciever_reset_queue(void)
{
	pbuf_rx_front = ReadReg(OHIO_INTERFACE_RX_WR);
	WriteReg(OHIO_INTERFACE_RX_RD, pbuf_rx_front);
}

u8 interface_send_msg_timeout(u8 type, u8 *pbuf, u8 len, int timeout_ms)
{
	u8 c, sending_len;
	u8 WriteDataBuf[32];

	/* full, return 0 */
	WriteDataBuf[0] = len + 1;	/* cmd */
	WriteDataBuf[1] = type;
	memcpy(WriteDataBuf + 2, pbuf, len);
	/* cmd + checksum */
	WriteDataBuf[len + 2] = cac_checksum(WriteDataBuf, len + 1 + 1);

	sending_len = WriteDataBuf[0] + 2;

	device_addr = OHIO_OCM_I2C_ADDR;
	c = ReadReg(OHIO_INTERFACE_TX_BUF);
	if (c == 0)
		WriteBlockReg(OHIO_INTERFACE_TX_BUF, sending_len, WriteDataBuf);
	else
		pr_info("Tx Buf Full\n");
	device_addr = OHIO_SLAVE_I2C_ADDR;

	return CMD_SUCCESS;
}

u8 try_source(void)
{
	u8 cc_status;
	unsigned long expire;

	pr_info("Try source start.\n");

	if (!(ReadReg(OHIO_POWER_CTRL) & 0x08)) {
		pr_info("Current role is DFP, no need Try source\n");
		return 1;
	}

	if (downstream_pd_cap)
		return interface_pr_swap();

	WriteReg(OHIO_OCM_CTRL, 0x10);
	WriteReg(0x4a, ReadReg(0x4a) | 0x01);
	WriteReg(0x6e, 0x01);
	WriteReg(OHIO_POWER_CTRL, ReadReg(OHIO_POWER_CTRL) | 0x02);

	expire = msecs_to_jiffies(600) + jiffies;
	for (;;) {
		cc_status = ReadReg(OHIO_CC_STATUS);
		if (cc_status & 0x0f)
			break;
		if (time_before(expire, jiffies)) {
			pr_info("Try source timeout!0x48 = %x\n", ReadReg(OHIO_CC_STATUS));
			pr_info("try source fail!\n");
			WriteReg(OHIO_POWER_CTRL, ReadReg(OHIO_POWER_CTRL) & 0xfd);
			WriteReg(OHIO_OCM_CTRL, 0x00);
			mdelay(1);
			WriteReg(0x69, 0x19);
			WriteReg(0x6e, 0x00);
			return 1;
		}
	}

	WriteReg(OHIO_OCM_CTRL, 0x00);
	mdelay(50);
	WriteReg(0x6e, 0x00);

	if (!(ReadReg(OHIO_POWER_CTRL) & 0x08)) {
		pr_info("try source swap success! \n");
		return 0;
	}

	pr_info("try source swap fail! \n");
	return 1;
}

u8 try_sink(void)
{
	unsigned long expire;

	pr_info("Try sink start.\n");

	if (ReadReg(OHIO_POWER_CTRL) & 0x08) {
		pr_info("Current role is UFP, no need Try sink\n");
		return 1;
	}

	if (downstream_pd_cap)
		return interface_pr_swap();

	WriteReg(OHIO_OCM_CTRL, 0x10);
	WriteReg(0x4a, ReadReg(0x4a) | 0x01);
	WriteReg(0x3f, ReadReg(0x3f) | 0xdf);
	WriteReg(0x36, ReadReg(0x36) | 0x80);
	WriteReg(OHIO_POWER_CTRL, ReadReg(OHIO_POWER_CTRL) & 0xfd);

	expire = msecs_to_jiffies(600) + jiffies;
	while (!(ReadReg(OHIO_ANALOG_STATUS) & 0xfc)) {
		if (time_before(expire, jiffies)) {
			pr_info("Try sink timeout!0x0d = %x\n", ReadReg(OHIO_ANALOG_STATUS));
			goto fail;
		}
	}

	mdelay(650);

	expire = msecs_to_jiffies(1200) + jiffies;
	while (!(ReadReg(OHIO_POWER_CTRL) & 0x10)) {
		if (time_before(expire, jiffies)) {
			pr_info("wait vbus timeout!0x40 = %x\n", ReadReg(OHIO_POWER_CTRL));
			goto fail;
		}
	}

	WriteReg(0x36, ReadReg(0x36) & 0x7f);
	WriteReg(OHIO_OCM_CTRL, 0x00);

	if (ReadReg(OHIO_POWER_CTRL) & 0x08) {
		pr_info("try sink swap success! \n");
		return 0;
	}

	pr_info("try sink  swap fail! \n");
	return 1;

fail:
	pr_info("try sink fail!\n");
	WriteReg(OHIO_POWER_CTRL, ReadReg(OHIO_POWER_CTRL) | 0x02);
	WriteReg(0x36, ReadReg(0x36) & 0x7f);
	WriteReg(OHIO_OCM_CTRL, 0x00);
	return 1;
}

u8 send_src_cap(const u8 *src_caps, u8 src_caps_size)
{
	if (NULL == src_caps)
		return CMD_FAIL;
	if ((src_caps_size % 4) != 0 || (src_caps_size / 4) > 7)
		return CMD_FAIL;

	memcpy(pd_src_pdo, src_caps, src_caps_size);
	pd_src_pdo_cnt = src_caps_size / 4;

	return interface_send_msg_timeout(TYPE_PWR_SRC_CAP, pd_src_pdo,
					  pd_src_pdo_cnt * 4, INTERFACE_TIMEOUT);
}

u8 send_snk_cap(const u8 *snk_caps, u8 snk_caps_size)
{
	memcpy(pd_snk_pdo, snk_caps, snk_caps_size);
	pd_snk_pdo_cnt = snk_caps_size / 4;

	return interface_send_msg_timeout(TYPE_PWR_SNK_CAP, pd_snk_pdo,
					  pd_snk_pdo_cnt * 4, INTERFACE_TIMEOUT);
}

u8 send_dp_snk_cfg(const u8 *dp_snk_caps, u8 dp_snk_caps_size)
{
	memcpy(configure_DP_caps, dp_snk_caps, dp_snk_caps_size);

	return interface_send_msg_timeout(TYPE_DP_SNK_CFG, configure_DP_caps, 4, INTERFACE_TIMEOUT);
}

u8 send_src_dp_cap(const u8 *dp_caps, u8 dp_caps_size)
{
	if (NULL == dp_caps)
		return CMD_FAIL;
	if ((dp_caps_size % 4) != 0 || (dp_caps_size / 4) > 7)
		return CMD_FAIL;

	memcpy(src_dp_caps, dp_caps, dp_caps_size);

	return interface_send_msg_timeout(TYPE_DP_SNK_IDENTITY, src_dp_caps,
					  dp_caps_size, INTERFACE_TIMEOUT);
}

u8 send_dp_snk_identity(const u8 *snk_ident, u8 snk_ident_size)
{
	return interface_send_msg_timeout(TYPE_DP_SNK_IDENTITY, (u8 *)snk_ident,
					  snk_ident_size, INTERFACE_TIMEOUT);
}

u8 send_vdm(const u8 *vdm, u8 size)
{
	u8 tmp[32] = { 0 };

	if (NULL == vdm)
		return CMD_FAIL;

	if (size > 3 && size < 32) {
		memcpy(tmp, vdm, size);
		if (tmp[2] == 0x01 && tmp[3] == 0x00) {
			tmp[3] = 0x40;
			return interface_send_msg_timeout(TYPE_VDM, tmp, size, INTERFACE_TIMEOUT);
		}
	}

	return CMD_REJECT;
}

u8 send_svid(const u8 *svid, u8 size)
{
	u8 tmp[4] = { 0 };

	if (NULL == svid || size != 4)
		return CMD_FAIL;

	memcpy(tmp, svid, 4);

	return interface_send_msg_timeout(TYPE_SVID, tmp, 4, INTERFACE_TIMEOUT);
}

u8 send_rdo(const u8 *rdo, u8 size)
{
	u8 i;

	if (NULL == rdo)
		return CMD_FAIL;
	if ((size % 4) != 0 || (size / 4) > 7)
		return CMD_FAIL;

	for (i = 0; i < size; i++)
		pd_rdo[i] = *rdo++;

	return interface_send_msg_timeout(TYPE_PWR_OBJ_REQ, pd_rdo, size, INTERFACE_TIMEOUT);
}

u8 send_power_swap(void)
{
	return interface_pr_swap();
}

u8 send_data_swap(void)
{
	return interface_dr_swap();
}

u8 send_accept(void)
{
	return interface_send_accept();
}

u8 send_reject(void)
{
	return interface_send_reject();
}

u8 send_soft_reset(void)
{
	return interface_send_soft_rst();
}

u8 send_hard_reset(void)
{
	return interface_send_hard_rst();
}

u8 polling_interface_msg(int timeout_ms)
{
	u8 ReadDataBuf[32];
	u8 global_i, checksum;

	device_addr = OHIO_OCM_I2C_ADDR;
	ReadBlockReg(OHIO_INTERFACE_RX_BUF, 32, ReadDataBuf);

	if (ReadDataBuf[0] != 0) {
		WriteReg(OHIO_INTERFACE_RX_BUF, 0);
		device_addr = OHIO_SLAVE_I2C_ADDR;

		checksum = 0;
		for (global_i = 0; global_i < ReadDataBuf[0] + 2; global_i++)
			checksum += ReadDataBuf[global_i];

		if (checksum == 0) {
			pr_info("\n>>%s\n", interface_to_str(ReadDataBuf[1]));
			dispatch_rcvd_pd_msg((PD_MSG_TYPE)ReadDataBuf[1], &ReadDataBuf[2],
					     ReadDataBuf[0] - 1);
			return CMD_SUCCESS;
		}

		pr_info("checksum error! n");
	}

	return CMD_FAIL;
}

u8 build_rdo_from_source_caps(u8 obj_cnt, u8 *buf)
{
	u8 i;
	u16 pdo_h, pdo_l, pdo_h_tmp, pdo_l_tmp;
	u32 pdo_max, voltage, max_voltage = 0;

	for (i = 0; i < (obj_cnt & 0x7); i++) {
		pdo_l = *(u16 *)buf;
		pdo_h = *(u16 *)(buf + 2);
		voltage = (((pdo_h & 0x0f) << 6) | (pdo_l >> 10)) * 50;
		if (voltage > max_voltage) {
			max_voltage = voltage;
			pdo_l_tmp = pdo_l;
			pdo_h_tmp = pdo_h;
			sel_voltage_pdo_index = i;
		}
		buf += 4;
	}
	pr_info("maxV=%d, cnt %d index %d\n", voltage, i, sel_voltage_pdo_index);

	if ((pdo_h_tmp & 0xc000) != 0x4000) {
		pdo_max = (pdo_l_tmp & 0x3ff) * 10;
		pr_info("maxMa %d\n", pdo_max);
		if (pdo_max < 900) {
			u32 rdo;

			pdo_max /= 10;
			rdo = (pdo_max << 10) | pdo_max;
			pd_rdo[0] = rdo & 0xff;
			pd_rdo[1] = (rdo >> 8) & 0xff;
			pd_rdo[2] = (rdo >> 16) & 0xff;
			pd_rdo[3] = ((((sel_voltage_pdo_index + 1) & 0x07) << 28) >> 24) | 0x04;
			return 1;
		}
	}

	/* default: 900mA operating / max current */
	pd_rdo[0] = 0x5a;
	pd_rdo[1] = 0x68;
	pd_rdo[2] = 0x01;
	pd_rdo[3] = ((sel_voltage_pdo_index + 1) & 0x07) << 4;
	return 1;
}

u32 change_bit_order(u8 *pbuf)
{
	return ((u32)pbuf[3] << 24) | ((u32)pbuf[2] << 16) | ((u32)pbuf[1] << 8) | pbuf[0];
}

u8 pd_check_requested_voltage(u32 rdo)
{
	int max_ma = rdo & 0x3ff;
	int op_ma = (rdo >> 10) & 0x3ff;
	int idx = rdo >> 28;
	u32 pdo, pdo_max;

	if (!idx || idx > pd_src_pdo_cnt) {
		pr_info("rdo = %x, Requested RDO is %d, Provided RDO number is %d\n",
			rdo, (unsigned int)idx, pd_src_pdo_cnt);
		return 0;
	}

	pdo = change_bit_order(pd_src_pdo + ((idx - 1) * 4));
	pdo_max = pdo & 0x3ff;
	pr_info("pdo_max = %x\n", pdo_max);

	if (op_ma > pdo_max)
		return 0;
	if (max_ma > pdo_max)
		return 0;

	return 1;
}

u8 recv_pd_source_caps_default_callback(void *para, u8 para_len)
{
	u8 ret;

	if ((para_len % 4) == 0 && build_rdo_from_source_caps(para_len / 4, para)) {
		ret = interface_send_request();
		pr_info("Snd RDO %x %x %x %x succ\n", pd_rdo[0], pd_rdo[1], pd_rdo[2], pd_rdo[3]);
		return ret;
	}

	return 1;
}

u8 recv_pd_sink_caps_default_callback(void *para, u8 para_len)
{
	if ((para_len % 4) != 0)
		return 0;
	if (para_len > 28)
		return 0;

	return 1;
}

u8 recv_pd_pwr_object_req_default_callback(void *para, u8 para_len)
{
	u8 *pdo = para;
	u32 rdo;

	if (para_len != 4)
		return 1;

	rdo = pdo[0] | (pdo[1] << 8) | (pdo[2] << 16) | (pdo[3] << 24);
	if (pd_check_requested_voltage(rdo))
		return send_accept();

	return interface_send_reject();
}

u8 recv_pd_accept_default_callback(void *para, u8 para_len)
{
	return 1;
}

u8 recv_pd_reject_default_callback(void *para, u8 para_len)
{
	return 1;
}

u8 recv_pd_goto_min_default_callback(void *para, u8 para_len)
{
	return 1;
}

void pd_vbus_control_default_func(bool on)
{
	pr_info("=====vbus control %d\n", on);

	/* stock flips dwc3 host/peripheral here through the FIH dwc3 ID hook; no host mode yet */
	pr_info("%s: VBUS source %s (host mode not supported yet)\n", LOG_TAG, on ? "on" : "off");
}

void pd_vconn_control_default_func(bool on)
{
}

void pd_cc_status_default_func(u8 cc_status)
{
	pr_info("cc status %x\n", cc_status);
}

void pd_drole_change_default_func(bool on)
{
}

void pd_got_rdo_change_default_func(struct anx7688_data *platform)
{
	u8 cc_status, cc1, cc2, rdo_max_voltage, rdo_max_power;
	int current_ma;

	cc_status = ReadReg(OHIO_NEW_CC_STATUS);
	cc2 = cc_status >> 4;
	cc1 = cc_status & 0x0f;
	rdo_max_voltage = ReadReg(OHIO_RDO_MAX_VOLTAGE);
	rdo_max_power = ReadReg(OHIO_RDO_MAX_POWER);

	pr_info("cc status: 0x%02X\n", cc_status);
	pr_info("rdomaxvol:0x%02X, rdomaxpwr:0x%02X\n", rdo_max_voltage, rdo_max_power);

	if (cc2 == 0x04 || cc1 == 0x04) {
		pr_info("SNK Default\n");
		current_ma = 900;
	} else if (cc2 == 0x08 || cc1 == 0x08) {
		pr_info("SNK Power1.5\n");
		current_ma = 1500;
	} else if (cc2 == 0x0c || cc1 == 0x0c) {
		pr_info("SNK Power3.0\n");
		current_ma = 3000;
	} else {
		pr_info("source type\n");
		current_ma = 0;
	}

	if (rdo_max_power)
		current_ma = rdo_max_power * 100;

	anx7688_set_current_capability(platform, current_ma);
}

void handle_intr_vector(struct anx7688_data *platform)
{
	u8 intr_vector, status;
	static u8 sys_sta_bak;

	intr_vector = ReadReg(OHIO_IRQ_EXT_SOURCE_2);
	pr_info(" intr vector = %x\n", intr_vector);
	WriteReg(OHIO_IRQ_EXT_SOURCE_2, 0);

	if (intr_vector & 0x01)
		polling_interface_msg(INTERACE_TIMEOUT_MS);

	if (intr_vector & 0x10)
		pd_cc_status_default_func(ReadReg(OHIO_NEW_CC_STATUS));

	status = ReadReg(OHIO_SYSTEM_STATUS);
	if ((intr_vector | (status ^ sys_sta_bak)) & 0x08)
		pd_vbus_control_default_func((ReadReg(OHIO_SYSTEM_STATUS) >> 3) & 0x01);

	if (intr_vector & 0x40)
		pd_got_rdo_change_default_func(platform);

	sys_sta_bak = status;
	WriteReg(OHIO_IRQ_SOURCE, 0x04);
}

u8 recv_pd_cmd_rsp_default_callback(void *para, u8 para_len)
{
	u8 *pdata = para;
	u8 result = pdata[1];

	switch (pdata[0]) {
	case TYPE_PSWAP_REQ:
		if (result == CMD_SUCCESS)
			pr_info("pd_cmd PRSwap result is successful\n");
		else if (result == CMD_REJECT)
			pr_info("pd_cmd PRSwap result is rejected\n");
		else if (result == CMD_BUSY)
			pr_info("pd_cmd PRSwap result is busy\n");
		else if (result == CMD_FAIL)
			pr_info("pd_cmd PRSwap result is fail\n");
		else
			pr_info("pd_cmd PRSwap result is unknown\n");
		break;
	case TYPE_DSWAP_REQ:
		if (result == CMD_SUCCESS)
			pr_info("pd_cmd DRSwap result is successful\n");
		else if (result == CMD_REJECT)
			pr_info("pd_cmd DRSwap result is rejected\n");
		else if (result == CMD_BUSY)
			pr_info("pd_cmd DRSwap result is busy\n");
		else if (result == CMD_FAIL)
			pr_info("pd_cmd DRSwap result is fail\n");
		else
			pr_info("pd_cmd DRSwap result is unknown\n");
		break;
	case TYPE_VCONN_SWAP_REQ:
		if (result == CMD_SUCCESS)
			pr_info("pd_cmd VCONN Swap result is successful\n");
		else if (result == CMD_REJECT)
			pr_info("pd_cmd VCONN Swap result is rejected\n");
		else if (result == CMD_BUSY)
			pr_info("pd_cmd VCONN Swap result is busy\n");
		else if (result == CMD_FAIL)
			pr_info("pd_cmd VCONN Swap result is fail\n");
		else
			pr_info("pd_cmd VCONN Swap result is unknown\n");
		break;
	case TYPE_PWR_OBJ_REQ:
		if (result == CMD_SUCCESS)
			pr_info("pd_cmd RDO request result is successful\n");
		else if (result == CMD_REJECT)
			pr_info("pd_cmd RDO reques result is rejected\n");
		else if (result == CMD_BUSY)
			pr_info("pd_cmd RDO reques result is busy\n");
		else if (result == CMD_FAIL)
			pr_info("pd_cmd RDO reques result is fail\n");
		else
			pr_info("pd_cmd RDO reques result is unknown\n");
		break;
	default:
		break;
	}

	return CMD_SUCCESS;
}

u8 recv_pd_hard_rst_default_callback(void *para, u8 para_len)
{
	pr_info("recv pd hard reset\n");
	return CMD_SUCCESS;
}

u8 recv_pd_dswap_default_callback(void *para, u8 para_len)
{
	return 1;
}

u8 recv_pd_pswap_default_callback(void *para, u8 para_len)
{
	return 1;
}

pd_callback_t get_pd_callback_fnc(PD_MSG_TYPE type)
{
	pd_callback_t fnc = 0;

	if (type < 256)
		fnc = pd_callback_array[type];

	return fnc;
}

void set_pd_callback_fnc(PD_MSG_TYPE type, pd_callback_t fnc)
{
	pd_callback_array[type] = fnc;
}

/* Never called; the u8 counter makes this loop forever, as in the original. */
void init_pd_msg_callback(void)
{
	u8 i;

	for (i = 0; i < 256; i++)
		pd_callback_array[i] = 0x0;
}

u8 send_pd_msg(PD_MSG_TYPE type, void *buf, u8 size)
{
	u8 rst = 0;

	switch (type) {
	case TYPE_PWR_SRC_CAP:
		rst = send_src_cap(buf, size);
		break;
	case TYPE_PWR_SNK_CAP:
		rst = send_snk_cap(buf, size);
		break;
	case TYPE_DP_SNK_IDENTITY:
		rst = interface_send_msg_timeout(TYPE_DP_SNK_IDENTITY, buf, size, INTERFACE_TIMEOUT);
		break;
	case TYPE_SVID:
		rst = send_svid(buf, size);
		break;
	case TYPE_GET_DP_SNK_CAP:
		rst = interface_send_msg_timeout(TYPE_GET_DP_SNK_CAP, 0, 0, INTERFACE_TIMEOUT);
		break;
	case TYPE_ACCEPT:
		rst = interface_send_accept();
		break;
	case TYPE_REJECT:
		rst = interface_send_reject();
		break;
	case TYPE_PSWAP_REQ:
		rst = send_power_swap();
		break;
	case TYPE_DSWAP_REQ:
		rst = send_data_swap();
		break;
	case TYPE_GOTO_MIN_REQ:
		rst = interface_send_msg_timeout(TYPE_GOTO_MIN_REQ, 0, 0, INTERFACE_TIMEOUT);
		break;
	case TYPE_VDM:
		rst = send_vdm(buf, size);
		break;
	case TYPE_DP_SNK_CFG:
		rst = send_dp_snk_cfg(buf, size);
		break;
	case TYPE_PWR_OBJ_REQ:
		rst = send_rdo(buf, size);
		break;
	case TYPE_SOFT_RST:
		rst = interface_send_soft_rst();
		break;
	case TYPE_HARD_RST:
		rst = interface_send_hard_rst();
		break;
	default:
		pr_info("unknown type %x\n", type);
		rst = 0;
		break;
	}

	if (rst == CMD_FAIL)
		pr_err("Cmd %x Fail.\n", type);

	return rst;
}

u8 dispatch_rcvd_pd_msg(PD_MSG_TYPE type, void *para, u8 para_len)
{
	u8 rst = 0;
	pd_callback_t fnc = get_pd_callback_fnc(type);

	if (fnc != 0) {
		rst = (*fnc)(para, para_len);
		return rst;
	}

	switch (type) {
	case TYPE_PWR_SRC_CAP:
		WriteReg(OHIO_FW_CTRL, 0xa0);
		downstream_pd_cap = 1;
		break;
	case TYPE_PWR_SNK_CAP:
		rst = recv_pd_sink_caps_default_callback(para, para_len);
		break;
	case TYPE_PWR_OBJ_REQ:
		WriteReg(OHIO_FW_CTRL, 0xa0);
		downstream_pd_cap = 1;
		break;
	case TYPE_ACCEPT:
		rst = recv_pd_accept_default_callback(para, para_len);
		break;
	case TYPE_PSWAP_REQ:
		rst = recv_pd_pswap_default_callback(para, para_len);
		break;
	case TYPE_DSWAP_REQ:
		rst = recv_pd_dswap_default_callback(para, para_len);
		break;
	case TYPE_RESPONSE_TO_REQ:
		rst = recv_pd_cmd_rsp_default_callback(para, para_len);
		break;
	case TYPE_HARD_RST:
		rst = recv_pd_hard_rst_default_callback(para, para_len);
		break;
	default:
		break;
	}

	return rst;
}

u8 register_pd_msg_callback_func(PD_MSG_TYPE type, pd_callback_t fnc)
{
	if (type > 256)
		return 1;

	set_pd_callback_fnc(type, fnc);
	return 0;
}
