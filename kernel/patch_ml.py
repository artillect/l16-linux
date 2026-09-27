# Apply the L16 changes to the mainline tree. Each edit is idempotent (checks its marker first).
# usage: python3 patch_ml.py <kernel tree>
import sys

T = sys.argv[1]


def edit(path, marker, anchor, text, after=True):
    p = T + '/' + path
    s = open(p).read()
    if marker in s:
        return
    assert s.count(anchor) == 1, '%s: expected exactly one %r' % (path, anchor)
    i = s.index(anchor) + (len(anchor) if after else 0)
    open(p, 'w').write(s[:i] + text + s[i:])
    print('patched', path)


def replace(path, marker, old, new):
    p = T + '/' + path
    s = open(p).read()
    if marker in s:
        return
    assert s.count(old) == 1, '%s: expected exactly one %r' % (path, old)
    open(p, 'w').write(s.replace(old, new))
    print('patched', path)


# Canary: Kconfig/Makefile, then the boot-stage hooks
edit('drivers/soc/qcom/Kconfig', 'config L16_CANARY', 'menu "Qualcomm SoC drivers"\n',
     '\nconfig L16_CANARY\n'
     '\tbool "Light L16 bring-up canary"\n'
     '\thelp\n'
     '\t  PS_HOLD panic reset and l16canary=<stage> boot-stage resets.\n')
edit('drivers/soc/qcom/Makefile', 'l16_canary.o', 'obj-$(CONFIG_QCOM_UBWC_CONFIG) += ubwc_config.o\n',
     'obj-$(CONFIG_L16_CANARY)\t+= l16_canary.o\n')

edit('arch/arm64/kernel/setup.c', 'l16_canary', '\tparse_early_param();\n',
     '\tl16_canary_stage(L16_CANARY_SETUP_ARCH);\n')
edit('arch/arm64/kernel/setup.c', 'linux/l16_canary.h', '#include <linux/acpi.h>\n',
     '#include <linux/l16_canary.h>\n')

edit('init/main.c', 'linux/l16_canary.h', '#include <linux/types.h>\n',
     '#include <linux/l16_canary.h>\n')
edit('init/main.c', 'L16_CANARY_START_KERNEL', '\trest_init();\n',
     '\tl16_canary_stage(L16_CANARY_START_KERNEL);\n', after=False)
edit('init/main.c', 'L16_CANARY_INITCALL', '\t\tdo_initcall_level(level, command_line);\n',
     '\t\tl16_canary_stage(L16_CANARY_INITCALL(level));\n', after=False)
edit('init/main.c', 'l16_canary_initcall', '\tif (initcall_blacklisted(fn))\n\t\treturn -EPERM;\n',
     '\n\tl16_canary_initcall(fn);\n')
edit('init/main.c', 'L16_CANARY_INITCALLS_DONE', '\tdo_basic_setup();\n',
     '\tl16_canary_stage(L16_CANARY_INITCALLS_DONE);\n')

# Canary: l16canary=100000+N logs every driver probe and resets before the Nth
edit('drivers/base/dd.c', 'linux/l16_canary.h', '#include <linux/device.h>\n',
     '#include <linux/l16_canary.h>\n')
edit('drivers/base/dd.c', 'l16_canary_probe', '\tint ret, link_ret;\n\n\tif (defer_all_probes) {\n',
     '\tl16_canary_probe(dev, drv->name);\n', after=False)

# SMBCHG input-current cap (see smbchg_patch.py)
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import smbchg_patch
smbchg_patch.apply(edit, replace)
import jeita_patch
jeita_patch.apply(edit, replace)

# Display: L16 Innolux NT35695 panel and the stock-programmed LM3630 backlight
edit('drivers/gpu/drm/panel/Kconfig', 'DRM_PANEL_INNOLUX_NT35695_L16', 'config DRM_PANEL_KHADAS_TS050\n',
     'config DRM_PANEL_INNOLUX_NT35695_L16\n'
     '\ttristate "Innolux NT35695 command-mode panel of the Light L16"\n'
     '\tdepends on OF\n'
     '\tdepends on DRM_MIPI_DSI\n'
     '\tdepends on BACKLIGHT_CLASS_DEVICE\n'
     '\thelp\n'
     '\t  1080x1920 DSI command-mode panel of the Light L16 camera.\n\n', after=False)
edit('drivers/gpu/drm/panel/Makefile', 'panel-innolux-nt35695-l16.o',
     'obj-$(CONFIG_DRM_PANEL_JDI_R63452) += panel-jdi-fhd-r63452.o\n',
     'obj-$(CONFIG_DRM_PANEL_INNOLUX_NT35695_L16) += panel-innolux-nt35695-l16.o\n')
edit('drivers/video/backlight/Kconfig', 'config BACKLIGHT_LM3630_L16', 'config BACKLIGHT_LM3630A\n',
     'config BACKLIGHT_LM3630_L16\n'
     '\ttristate "TI LM3630 backlight of the Light L16 (stock register values)"\n'
     '\tdepends on I2C\n'
     '\tselect REGMAP_I2C\n'
     '\thelp\n'
     '\t  LM3630 programmed exactly as the Light L16 stock kernel does.\n\n', after=False)
edit('drivers/video/backlight/Makefile', 'lm3630_bl.o',
     'obj-$(CONFIG_BACKLIGHT_LM3630A)\t\t+= lm3630a_bl.o\n',
     'obj-$(CONFIG_BACKLIGHT_LM3630_L16)\t+= lm3630_bl.o\n')

# drivers/usb/misc: ANX7688 Type-C controller (mainline port of the stock L16 driver)
edit('drivers/usb/misc/Kconfig', 'anx7688/Kconfig', '\nconfig USB_EMI62',
     '\nsource "drivers/usb/misc/anx7688/Kconfig"\n', after=False)
edit('drivers/usb/misc/Makefile', 'anx7688/', 'obj-$(CONFIG_USB_EMI62)',
     'obj-$(CONFIG_USB_TYPE_C_ANX7688)\t+= anx7688/\n', after=False)

# u_ether: ifdown/ifup (e.g. NetworkManager taking over usb0) disables and re-enables the
# data endpoints while the host is still connected. On the L16's dwc3 the bulk-IN endpoint
# never runs again afterwards (TRBs stay hardware-owned, TX stalls until the gadget is
# re-bound). While the host is connected, let queued transfers finish instead, as the
# driver's own REVISIT note suggests.
replace('drivers/usb/gadget/function/u_ether.c', 'L16: keep endpoints',
        '\t\tin = link->in_ep->desc;\n'
        '\t\tout = link->out_ep->desc;\n'
        '\t\tusb_ep_disable(link->in_ep);\n'
        '\t\tusb_ep_disable(link->out_ep);\n'
        '\t\tif (netif_carrier_ok(net)) {\n'
        '\t\t\tDBG(dev, "host still using in/out endpoints\\n");\n'
        '\t\t\tlink->in_ep->desc = in;\n'
        '\t\t\tlink->out_ep->desc = out;\n'
        '\t\t\tusb_ep_enable(link->in_ep);\n'
        '\t\t\tusb_ep_enable(link->out_ep);\n'
        '\t\t}\n',
        '\t\t/* L16: keep endpoints running while the host is connected */\n'
        '\t\tif (!netif_carrier_ok(net)) {\n'
        '\t\t\tusb_ep_disable(link->in_ep);\n'
        '\t\t\tusb_ep_disable(link->out_ep);\n'
        '\t\t}\n')
replace('drivers/usb/gadget/function/u_ether.c', 'L16: no saved descriptors',
        '\t\tstruct gether\t*link = dev->port_usb;\n'
        '\t\tconst struct usb_endpoint_descriptor *in;\n'
        '\t\tconst struct usb_endpoint_descriptor *out;\n',
        '\t\tstruct gether\t*link = dev->port_usb; /* L16: no saved descriptors */\n')

# drivers/input/misc: L16 touch strip (Elan eKTF, one-dimensional)
edit('drivers/input/misc/Kconfig', 'INPUT_L16_TOUCHSTRIP', 'config INPUT_GPIO_BEEPER\n',
     'config INPUT_L16_TOUCHSTRIP\n'
     '\ttristate "Light L16 touch strip"\n'
     '\tdepends on I2C\n'
     '\thelp\n'
     '\t  One-dimensional Elan eKTF touch strip of the Light L16 camera.\n\n', after=False)
edit('drivers/input/misc/Makefile', 'l16-touchstrip.o',
     'obj-$(CONFIG_INPUT_GPIO_BEEPER)\t\t+= gpio-beeper.o\n',
     'obj-$(CONFIG_INPUT_L16_TOUCHSTRIP)\t+= l16-touchstrip.o\n')

# MDP5 mixer choice: upstream keeps the *last* pair-able layer mixer, i.e. LM2/PP2 on
# MSM8996. The L16's panel TE (GPIO 10, mdp_vsync) is pulsing at 60 Hz, yet on PP2 every
# few frames hit "pp done time out" and commits fail with EBUSY. Stock drives DSI0 from
# LM0/PP0, so take the *first* pair-able mixer instead.
replace('drivers/gpu/drm/msm/disp/mdp5/mdp5_mixer.c', 'L16: first pair-able',
        '\t\tif (!(*mixer) || cur->caps & MDP_LM_CAP_PAIR)\n'
        '\t\t\t*mixer = cur;\n',
        '\t\t/* L16: first pair-able mixer (LM0/PP0, as stock uses for DSI0) */\n'
        '\t\tif (!(*mixer) || (cur->caps & MDP_LM_CAP_PAIR &&\n'
        '\t\t\t\t  !((*mixer)->caps & MDP_LM_CAP_PAIR)))\n'
        '\t\t\t*mixer = cur;\n')

# MDP5 suspend/resume: drm_atomic_helper_suspend() saves plane/CRTC/connector states but
# not the mdp5 global (private object) state. The suspend commit releases every hwpipe
# (hwpipe_to_plane[] = NULL); on resume the saved plane states still point at their old
# hwpipes, atomic_check keeps them without re-registering, and the next release trips
# WARN_ON(!hwpipe_to_plane[idx]) -> -EINVAL: the display can no longer be disabled and
# the next suspend aborts (msm_kms_pm_prepare -22). If a kept hwpipe is not registered
# to this plane, drop it and take the normal assign path.
replace('drivers/gpu/drm/msm/disp/mdp5/mdp5_plane.c', 'L16: stale hwpipe',
        '\t\t/* (re)assign hwpipe if needed, otherwise keep old one: */\n'
        '\t\tif (new_hwpipe) {\n',
        '\t\t/* L16: stale hwpipe (plane state restored after suspend) */\n'
        '\t\tif (!new_hwpipe && mdp5_state->hwpipe) {\n'
        '\t\t\tstruct mdp5_global_state *gs = mdp5_get_global_state(state->state);\n'
        '\n'
        '\t\t\tif (IS_ERR(gs))\n'
        '\t\t\t\treturn PTR_ERR(gs);\n'
        '\t\t\tif (gs->hwpipe.hwpipe_to_plane[mdp5_state->hwpipe->idx] != plane) {\n'
        '\t\t\t\tmdp5_state->hwpipe = NULL;\n'
        '\t\t\t\tif (mdp5_state->r_hwpipe &&\n'
        '\t\t\t\t    gs->hwpipe.hwpipe_to_plane[mdp5_state->r_hwpipe->idx] != plane)\n'
        '\t\t\t\t\tmdp5_state->r_hwpipe = NULL;\n'
        '\t\t\t\tnew_hwpipe = true;\n'
        '\t\t\t}\n'
        '\t\t}\n'
        '\n'
        '\t\t/* (re)assign hwpipe if needed, otherwise keep old one: */\n'
        '\t\tif (new_hwpipe) {\n')

# PCIe0 across suspend: the PCIE0 GDSC is PWRSTS_OFF_ON, so genpd switches it off at
# system suspend (noirq) even though qcom-pcie keeps its resources on while a link is up.
# The QCA6174 then comes back "D3cold ... device inaccessible" and any access hangs the
# bus. Newer Qualcomm platforms declare their PCIe GDSCs PWRSTS_RET_ON (a software
# disable leaves the domain on; hardware may only retain it), which keeps the link.
replace('drivers/clk/qcom/gcc-msm8996.c', 'L16: keep PCIe0 link',
        '\t\t.name = "pcie0",\n\t},\n\t.pwrsts = PWRSTS_OFF_ON,\n',
        '\t\t.name = "pcie0",\n\t},\n\t/* L16: keep PCIe0 link across suspend (as newer SoCs do) */\n'
        '\t.pwrsts = PWRSTS_RET_ON,\n')

# Power key during suspend: the key only becomes a wakeup interrupt in the driver's own
# suspend callback, so a press between the task freeze and that point was delivered as a
# plain key event: the suspend went ahead, and Phosh read the stale press after the next
# wake and blanked the screen again. Report presses as hard wakeup events, which abort a
# suspend in progress (the PM core clears pending aborts when it starts freezing tasks,
# so this does nothing during normal use).
replace('drivers/input/misc/pm8941-pwrkey.c', 'L16: press aborts suspend',
        '\tpwrkey->last_status = sts;\n',
        '\tpwrkey->last_status = sts;\n'
        '\n'
        '\t/* L16: press aborts suspend */\n'
        '\tif (sts && device_may_wakeup(pwrkey->dev))\n'
        '\t\tpm_wakeup_hard_event(pwrkey->dev);\n')

# PMI8994 fuel gauge (gen1): the SRAM battery current is positive when discharging; the
# power_supply ABI (and UPower) want negative for discharging. Measured on the L16:
# +240 mA on battery, -250 mA charging from USB.
replace('drivers/power/supply/qcom_fg.c', 'L16: discharge is negative',
        '\t*val = div_s64((s64)temp * 152587, 1000);\n',
        '\t/* L16: discharge is negative (the FG reports it positive) */\n'
        '\t*val = -div_s64((s64)temp * 152587, 1000);\n')

# drivers/input/misc: DW7800 haptics (L16 vibration motor)
edit('drivers/input/misc/Kconfig', 'INPUT_DW7800_HAPTICS', 'config INPUT_GPIO_BEEPER\n',
     'config INPUT_DW7800_HAPTICS\n'
     '\ttristate "Dongwoon DW7800 haptics"\n'
     '\tdepends on I2C\n'
     '\tselect INPUT_FF_MEMLESS\n'
     '\thelp\n'
     '\t  Dongwoon DW7800 FIFO haptic driver (Light L16 vibration motor).\n\n', after=False)
edit('drivers/input/misc/Makefile', 'dw7800-haptics.o',
     'obj-$(CONFIG_INPUT_GPIO_BEEPER)\t\t+= gpio-beeper.o\n',
     'obj-$(CONFIG_INPUT_DW7800_HAPTICS)\t+= dw7800-haptics.o\n')

# qcom_smgr: the buffering report's metadata TLV (0x02) is a counted array with one
# 12-byte entry per requested item (13 bytes for one item, 25 for two). Upstream
# decodes it as a single 13-byte struct, which only fits one item; nothing uses it.
replace('drivers/iio/common/qcom_smgr/qmi/qmi_sns_smgr.h', 'L16: one metadata entry per item',
        '\tstruct sns_smgr_buffering_report_metadata metadata;\n',
        '\tu8 metadata_len; /* L16: one metadata entry per item */\n'
        '\tstruct sns_smgr_buffering_report_item_meta metadata[2];\n')
edit('drivers/iio/common/qcom_smgr/qmi/qmi_sns_smgr.h', 'struct sns_smgr_buffering_report_item_meta {',
     'struct sns_smgr_buffering_report_ind {\n',
     'struct sns_smgr_buffering_report_item_meta {\n'
     '\tu8 raw[12];\n'
     '};\n\n', after=False)
edit('drivers/iio/common/qcom_smgr/qmi/qmi_sns_smgr.c', 'sns_smgr_buffering_report_item_meta_ei',
     'const struct qmi_elem_info sns_smgr_buffering_report_ind_ei[] = {\n',
     'static const struct qmi_elem_info sns_smgr_buffering_report_item_meta_ei[] = {\n'
     '\t{\n'
     '\t\t.data_type = QMI_UNSIGNED_1_BYTE,\n'
     '\t\t.elem_len = 12,\n'
     '\t\t.elem_size = sizeof(u8),\n'
     '\t\t.array_type = STATIC_ARRAY,\n'
     '\t\t.offset = offsetof(struct sns_smgr_buffering_report_item_meta, raw),\n'
     '\t},\n'
     '\t{\n'
     '\t\t.data_type = QMI_EOTI,\n'
     '\t},\n'
     '};\n\n', after=False)
replace('drivers/iio/common/qcom_smgr/qmi/qmi_sns_smgr.c', 'L16: metadata array',
        '\t{\n'
        '\t\t.data_type = QMI_STRUCT,\n'
        '\t\t.elem_len = 1,\n'
        '\t\t.elem_size = sizeof_field(struct sns_smgr_buffering_report_ind,\n'
        '\t\t\t\t\t  metadata),\n'
        '\t\t.array_type = NO_ARRAY,\n'
        '\t\t.tlv_type = 0x02,\n'
        '\t\t.offset = offsetof(struct sns_smgr_buffering_report_ind,\n'
        '\t\t\t\t   metadata),\n'
        '\t\t.ei_array = sns_smgr_buffering_report_metadata_ei,\n'
        '\t},\n',
        '\t{\n'
        '\t\t/* L16: metadata array, one entry per requested item */\n'
        '\t\t.data_type = QMI_DATA_LEN,\n'
        '\t\t.elem_len = 1,\n'
        '\t\t.elem_size = sizeof(u8),\n'
        '\t\t.array_type = NO_ARRAY,\n'
        '\t\t.tlv_type = 0x02,\n'
        '\t\t.offset = offsetof(struct sns_smgr_buffering_report_ind,\n'
        '\t\t\t\t   metadata_len),\n'
        '\t},\n'
        '\t{\n'
        '\t\t.data_type = QMI_STRUCT,\n'
        '\t\t.elem_len = 2,\n'
        '\t\t.elem_size = sizeof(struct sns_smgr_buffering_report_item_meta),\n'
        '\t\t.array_type = VAR_LEN_ARRAY,\n'
        '\t\t.tlv_type = 0x02,\n'
        '\t\t.offset = offsetof(struct sns_smgr_buffering_report_ind,\n'
        '\t\t\t\t   metadata),\n'
        '\t\t.ei_array = sns_smgr_buffering_report_item_meta_ei,\n'
        '\t},\n')

# qcom_smgr prox/light (L16: Sensortek stk3x1x): the proximity half never sees a
# reflection (raw 1-2 counts, always "far", even face down on a table), the light half
# works (lux, Q16, front side). Stream the secondary data type (light) instead and expose
# it as in_intensity_both, which iio-sensor-proxy uses as a buffered ambient light sensor.
edit('drivers/iio/common/qcom_smgr/qcom_smgr.c', 'L16: light, not proximity',
     '\t\treq.items[0].val2 = 1;\n',
     '\n'
     '\t\t/* L16: light, not proximity (secondary data type of the prox/light sensor) */\n'
     '\t\tif (sensor->type == SNS_SMGR_SENSOR_TYPE_PROX_LIGHT &&\n'
     '\t\t    sensor->data_type_count > 1)\n'
     '\t\t\treq.items[0].data_type = SNS_SMGR_DATA_TYPE_SECONDARY;\n')
replace('drivers/iio/common/qcom_smgr/qcom_smgr.c', 'L16: lux passes through',
        '\tif (sensor->type == SNS_SMGR_SENSOR_TYPE_PROX_LIGHT) {\n'
        '\t\ttemp = le32_to_cpu(iio_data.values[0]);\n',
        '\t/* L16: lux passes through; no proximity conversion */\n'
        '\tif (0) {\n'
        '\t\ttemp = le32_to_cpu(iio_data.values[0]);\n')
replace('drivers/iio/common/qcom_smgr/qcom_smgr.c', 'L16: ambient light',
        '\t\t.type = IIO_PROXIMITY,\n'
        '\t\t.scan_index = 0,\n',
        '\t\t.type = IIO_INTENSITY, /* L16: ambient light */\n'
        '\t\t.modified = true,\n'
        '\t\t.channel2 = IIO_MOD_LIGHT_BOTH,\n'
        '\t\t.scan_index = 0,\n')

# wcd9335: capture (decimator) digital gain controls, as wcd934x has. The hardware has
# them (TXn_TX_VOL_CTL, -84..+40 dB, the decimator enable re-applies the value) but the
# driver exposes none, and the L16's DMICs are ~20 dB too quiet at 0 dB.
edit('sound/soc/codecs/wcd9335.c', 'DEC0 Volume',
     'static const struct snd_kcontrol_new wcd9335_snd_controls[] = {\n',
     ''.join('\tSOC_SINGLE_S8_TLV("DEC%d Volume", WCD9335_CDC_TX0_TX_VOL_CTL + 16 * %d,\n'
             '\t\t\t-84, 40, digital_gain),\n' % (n, n) for n in range(9)))
