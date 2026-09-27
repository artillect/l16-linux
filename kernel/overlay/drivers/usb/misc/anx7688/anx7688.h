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

#ifndef __ANX7688_H__
#define __ANX7688_H__

#include <linux/types.h>
#include <linux/i2c.h>
#include <linux/mutex.h>
#include <linux/power_supply.h>
#include <linux/regulator/consumer.h>
#include <linux/gpio/consumer.h>
#include <linux/workqueue.h>

typedef unsigned char unchar;

#define LOG_TAG			"Anx7688"
#define ANX_DRV_VERSION		"2.1.11"

/* I2C slave addresses (8-bit form, as the reference driver keeps them) */
#define OHIO_SLAVE_I2C_ADDR	0x50
#define OHIO_OCM_I2C_ADDR	0x58

/* OHIO_SLAVE_I2C_ADDR registers */
#define OHIO_CHIP_ID_L		0x02
#define OHIO_CHIP_ID_H		0x03
#define OHIO_CHIP_REV		0x04
#define OHIO_OCM_CTRL		0x05
#define OHIO_ANALOG_STATUS	0x0d
#define OHIO_OCM_LOAD_STATUS	0x12
#define OHIO_INTERFACE_RX_RD	0x13
#define OHIO_INTERFACE_RX_WR	0x14
#define OHIO_OCM_FW_VER_MAJOR	0x15
#define OHIO_OCM_FW_VER_MINOR	0x16
#define OHIO_MAX_VOLTAGE	0x1b
#define OHIO_MAX_POWER		0x1c
#define OHIO_MIN_POWER		0x1d
#define OHIO_RDO_MAX_VOLTAGE	0x1e
#define OHIO_RDO_MAX_POWER	0x1f
#define OHIO_FW_CTRL		0x22
#define OHIO_FW_CTRL_2		0x23
#define OHIO_FW_CTRL_3		0x27
#define OHIO_IRQ_EXT_SOURCE_2	0x28
#define OHIO_SYSTEM_STATUS	0x29
#define OHIO_NEW_CC_STATUS	0x2a
#define OHIO_INTERFACE_TX_BUF	0x30
#define OHIO_POWER_CTRL		0x40
#define OHIO_CC_STATUS		0x48
#define OHIO_IRQ_SOURCE		0x4f
#define OHIO_INTERFACE_RX_BUF	0x51

/* OHIO_OCM_I2C_ADDR registers */
#define OHIO_OCM_IRQ		0x10
#define OHIO_DP_PIN_ASSIGN	0x85
#define OHIO_DP_SIGNALING	0x86

#define INTERFACE_TIMEOUT	30
#define INTERACE_TIMEOUT_MS	26

enum {
	CMD_SUCCESS,
	CMD_REJECT,
	CMD_FAIL,
	CMD_BUSY,
};

typedef enum {
	TYPE_PWR_SRC_CAP = 0x00,
	TYPE_PWR_SNK_CAP = 0x01,
	TYPE_DP_SNK_IDENTITY = 0x02,
	TYPE_SVID = 0x03,
	TYPE_GET_DP_SNK_CAP = 0x04,
	TYPE_ACCEPT = 0x05,
	TYPE_REJECT = 0x06,
	TYPE_PSWAP_REQ = 0x10,
	TYPE_DSWAP_REQ = 0x11,
	TYPE_GOTO_MIN_REQ = 0x12,
	TYPE_VCONN_SWAP_REQ = 0x13,
	TYPE_VDM = 0x14,
	TYPE_DP_SNK_CFG = 0x15,
	TYPE_PWR_OBJ_REQ = 0x16,
	TYPE_PD_STATUS_REQ = 0x17,
	TYPE_DP_ALT_ENTER = 0x19,
	TYPE_DP_ALT_EXIT = 0x1a,
	TYPE_RESPONSE_TO_REQ = 0xf0,
	TYPE_SOFT_RST = 0xf1,
	TYPE_HARD_RST = 0xf2,
	TYPE_RESTART = 0xf3,
} PD_MSG_TYPE;

typedef u8 (*pd_callback_t)(void *para, u8 para_len);

/* Convenience wrappers the reference driver keeps as macros. */
#define interface_pr_swap() \
	interface_send_msg_timeout(TYPE_PSWAP_REQ, 0, 0, INTERFACE_TIMEOUT)
#define interface_dr_swap() \
	interface_send_msg_timeout(TYPE_DSWAP_REQ, 0, 0, INTERFACE_TIMEOUT)
#define interface_send_accept() \
	interface_send_msg_timeout(TYPE_ACCEPT, 0, 0, INTERFACE_TIMEOUT)
#define interface_send_reject() \
	interface_send_msg_timeout(TYPE_REJECT, 0, 0, INTERFACE_TIMEOUT)
#define interface_send_soft_rst() \
	interface_send_msg_timeout(TYPE_SOFT_RST, 0, 0, INTERFACE_TIMEOUT)
#define interface_send_hard_rst() \
	interface_send_msg_timeout(TYPE_HARD_RST, 0, 0, INTERFACE_TIMEOUT)
#define interface_send_request() \
	interface_send_msg_timeout(TYPE_PWR_OBJ_REQ, pd_rdo, 4, INTERFACE_TIMEOUT)

struct anx7688_platform_data {
	struct gpio_desc *gpio_p_on;
	struct gpio_desc *gpio_reset;
	struct gpio_desc *gpio_cbl_det;
	struct gpio_desc *gpio_intr_comm;
	struct gpio_desc *gpio_dp_ldo_on;
	int cbl_det_status;
	struct regulator *vconn_5v;
	struct regulator *hsusb_1p8;
};

struct anx7688_data {
	struct anx7688_platform_data *pdata;
	struct delayed_work work;
	struct workqueue_struct *workqueue;
	struct mutex lock;
	struct i2c_client *client;
	struct power_supply *chg_psy;	/* mainline qcom-smbchg USB input */
	int current_capability;
};

/* anx7688_driver.c */
extern unchar device_addr;
extern unchar debug_on;
extern unchar ocm_bootload_done;
extern int anx7688_power_status;
extern struct i2c_client *anx7688_client;

unchar ReadReg(unchar RegAddr);
int ReadBlockReg(unchar RegAddr, unchar len, unchar *dat);
int WriteBlockReg(unchar RegAddr, unchar len, const unchar *dat);
void WriteReg(unchar RegAddr, unchar RegVal);
void MI1_power_on(void);
void anx7688_hardware_reset(int enable);
void anx7688_power_standby(void);
void anx7688_hardware_poweron(void);
void anx7688_vbus_control(bool value);
void anx7688_main_process(void);
void dump_reg(void);
int get_disport_capability(unchar *dp_pin_assign, unchar *dp_signaling);

/* anx7688_interface.c */
extern u8 pbuf_rx_front;
extern u8 pbuf_tx_rear;
extern u8 downstream_pd_cap;
extern u8 pd_src_pdo[];
extern u8 pd_src_pdo_cnt;
extern u8 pd_snk_pdo[];
extern u8 pd_snk_pdo_cnt;
extern u8 pd_rdo[];
extern u8 sel_voltage_pdo_index;
extern u8 configure_DP_caps[];
extern u8 src_dp_caps[];

u8 get_otp_indicator_byte(void);
u8 get_data_role(void);
u8 get_power_role(void);
u8 get_src_cap(const u8 *src_caps, u8 src_caps_size);
u8 get_snk_cap(const u8 *snk_caps, u8 snk_caps_size);
char *interface_to_str(unsigned char header_type);
u8 cac_checksum(u8 *pSendBuf, u8 len);
void printb(const char *buf, size_t size);
void interface_init(void);
void send_initialized_setting(void);
void chip_register_init(void);
void reciever_reset_queue(void);
u8 interface_send_msg_timeout(u8 type, u8 *pbuf, u8 len, int timeout_ms);
u8 try_source(void);
u8 try_sink(void);
u8 send_src_cap(const u8 *src_caps, u8 src_caps_size);
u8 send_snk_cap(const u8 *snk_caps, u8 snk_caps_size);
u8 send_dp_snk_cfg(const u8 *dp_snk_caps, u8 dp_snk_caps_size);
u8 send_src_dp_cap(const u8 *dp_caps, u8 dp_caps_size);
u8 send_dp_snk_identity(const u8 *snk_ident, u8 snk_ident_size);
u8 send_vdm(const u8 *vdm, u8 size);
u8 send_svid(const u8 *svid, u8 size);
u8 send_rdo(const u8 *rdo, u8 size);
u8 send_power_swap(void);
u8 send_data_swap(void);
u8 send_accept(void);
u8 send_reject(void);
u8 send_soft_reset(void);
u8 send_hard_reset(void);
u8 polling_interface_msg(int timeout_ms);
u8 build_rdo_from_source_caps(u8 obj_cnt, u8 *buf);
u32 change_bit_order(u8 *pbuf);
u8 pd_check_requested_voltage(u32 rdo);
u8 recv_pd_source_caps_default_callback(void *para, u8 para_len);
u8 recv_pd_sink_caps_default_callback(void *para, u8 para_len);
u8 recv_pd_pwr_object_req_default_callback(void *para, u8 para_len);
u8 recv_pd_accept_default_callback(void *para, u8 para_len);
u8 recv_pd_reject_default_callback(void *para, u8 para_len);
u8 recv_pd_goto_min_default_callback(void *para, u8 para_len);
void pd_vbus_control_default_func(bool on);
void pd_vconn_control_default_func(bool on);
void pd_cc_status_default_func(u8 cc_status);
void pd_drole_change_default_func(bool on);
void pd_got_rdo_change_default_func(struct anx7688_data *platform);
void handle_intr_vector(struct anx7688_data *platform);
void anx7688_set_current_capability(struct anx7688_data *platform, int current_ma);
u8 recv_pd_cmd_rsp_default_callback(void *para, u8 para_len);
u8 recv_pd_hard_rst_default_callback(void *para, u8 para_len);
u8 recv_pd_dswap_default_callback(void *para, u8 para_len);
u8 recv_pd_pswap_default_callback(void *para, u8 para_len);
pd_callback_t get_pd_callback_fnc(PD_MSG_TYPE type);
void set_pd_callback_fnc(PD_MSG_TYPE type, pd_callback_t fnc);
void init_pd_msg_callback(void);
u8 send_pd_msg(PD_MSG_TYPE type, void *buf, u8 size);
u8 dispatch_rcvd_pd_msg(PD_MSG_TYPE type, void *para, u8 para_len);
u8 register_pd_msg_callback_func(PD_MSG_TYPE type, pd_callback_t fnc);


#endif /* __ANX7688_H__ */
