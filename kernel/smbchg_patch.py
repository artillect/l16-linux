# SMBCHG: optional "input-current-limit-microamp" caps the AICL ceiling (upstream uses the
# top of the ICL table). The L16's stock kernel limits USB input to 2000 mA.
# Imported by patch_ml.py (kept separate so the C text isn't mangled by shell quoting).


def apply(edit, replace):
    edit('drivers/power/supply/qcom-smbchg.h', 'ilim_max_ua',
         '\tstruct power_supply_battery_info *batt_info;\n',
         '\tint ilim_max_ua;\n')
    replace('drivers/power/supply/qcom-smbchg.c', 'chip->ilim_max_ua ?',
            '\t\t\t\t\t USBIN_INPUT_MASK,\n'
            '\t\t\t\t\t chip->data->ilim_table_len - 1);\n',
            '\t\t\t\t\t USBIN_INPUT_MASK,\n'
            '\t\t\t\t\t chip->ilim_max_ua ?\n'
            '\t\t\t\t\t find_closest_smaller(chip->ilim_max_ua, chip->data->ilim_table,\n'
            '\t\t\t\t\t\t\t      (unsigned int)chip->data->ilim_table_len) :\n'
            '\t\t\t\t\t chip->data->ilim_table_len - 1);\n')
    # PC port (SDP): go back to low-current mode at 500 mA like stock. Upstream leaves
    # high-current mode (and the AICL limit) set after a DCP/CDP session, so a later SDP
    # connection could draw whatever AICL finds.
    edit('drivers/power/supply/qcom-smbchg.c', 'L16: SDP',
         '\tusb_type = smbchg_usb_get_type(chip);\n',
         '\n\t/* L16: SDP gets the USB 2.0 configured current in low-current mode */\n'
         '\tif (usb_present && usb_type == POWER_SUPPLY_USB_TYPE_SDP)\n'
         '\t\tsmbchg_usb_set_ilim_lc(chip, 500000);\n')
    edit('drivers/power/supply/qcom-smbchg.c', 'input-current-limit-microamp',
         '\tret = device_property_read_u32(chip->dev, "reg", &chip->base);\n',
         '\tdevice_property_read_u32(chip->dev, "input-current-limit-microamp",\n'
         '\t\t\t\t (u32 *)&chip->ilim_max_ua);\n', after=False)
