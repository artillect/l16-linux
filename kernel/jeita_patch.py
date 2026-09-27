# L16 JEITA patches (imported by patch_ml.py).
#
# qcom_fg: take the JEITA thresholds from the monitored battery instead of fixed defaults.
#   FG SRAM 0x454 offsets (checked against downstream qpnp-fg.c and the stock kernel's
#   settings[] table): 0 = soft cold, 1 = soft hot, 2 = hard cold, 3 = hard hot.
#   Mainline names them TEMP_MIN/TEMP_MAX/TEMP_ALERT_MIN/TEMP_ALERT_MAX in that offset order.
#   simple-battery: alert-celsius = <soft cold, soft hot>, operating-range-celsius =
#   <hard cold, hard hot>, in whole degrees C; the setter takes tenths of a degree (x10).
#
# qcom-smbchg: optional "qcom,fastchg-current-comp-microamp" programs FCC_CMP_CFG (0xf3,
#   bits 1:0), the charge current used in the soft JEITA zones. PMI8996 table (downstream
#   fcc_comp_table_8996): 250, 1100, 1200, 1500 mA.


def apply(edit, replace):
    f = 'drivers/power/supply/qcom_fg.c'
    edit(f, 'L16_JEITA', '#define BATT_TEMP_JEITA_HOT\t\t450\n',
         '\n/* L16: JEITA threshold from the monitored battery (deg C -> tenths), else the default */\n'
         '#define L16_JEITA(chip, field, unset, def) \\\n'
         '\t((chip)->batt_info->field != (unset) ? (chip)->batt_info->field * 10 : (def))\n')
    replace(f, 'L16_JEITA(chip, temp_alert_min',
            'POWER_SUPPLY_PROP_TEMP_MIN,\n\t\t\t\t\t\tBATT_TEMP_JEITA_COLD);',
            'POWER_SUPPLY_PROP_TEMP_MIN,\n\t\t\t\t\t\tL16_JEITA(chip, temp_alert_min, INT_MIN, BATT_TEMP_JEITA_COLD));')
    replace(f, 'L16_JEITA(chip, temp_alert_max',
            'POWER_SUPPLY_PROP_TEMP_MAX,\n\t\t\t\t\t\tBATT_TEMP_JEITA_WARM);',
            'POWER_SUPPLY_PROP_TEMP_MAX,\n\t\t\t\t\t\tL16_JEITA(chip, temp_alert_max, INT_MAX, BATT_TEMP_JEITA_WARM));')
    replace(f, 'L16_JEITA(chip, temp_min',
            'POWER_SUPPLY_PROP_TEMP_ALERT_MIN,\n\t\t\t\t\t\tBATT_TEMP_JEITA_COOL);',
            'POWER_SUPPLY_PROP_TEMP_ALERT_MIN,\n\t\t\t\t\t\tL16_JEITA(chip, temp_min, INT_MIN, BATT_TEMP_JEITA_COOL));')
    replace(f, 'L16_JEITA(chip, temp_max',
            'POWER_SUPPLY_PROP_TEMP_ALERT_MAX,\n\t\t\t\t\t\tBATT_TEMP_JEITA_HOT);',
            'POWER_SUPPLY_PROP_TEMP_ALERT_MAX,\n\t\t\t\t\t\tL16_JEITA(chip, temp_max, INT_MAX, BATT_TEMP_JEITA_HOT));')

    c = 'drivers/power/supply/qcom-smbchg.c'
    replace(c, 'fastchg-current-comp-microamp',
            '\t/* Set constant charge current limit */\n',
            '\t/* L16: soft-JEITA charge current (FCC_CMP_CFG), PMI8996 only */\n'
            '\tif (chip->data == &smbchg_pmi8996_data) {\n'
            '\t\tstatic const int fcc_comp_ua[] = { 250000, 1100000, 1200000, 1500000 };\n'
            '\t\tu32 comp;\n'
            '\t\tint i;\n'
            '\n'
            '\t\tif (!device_property_read_u32(chip->dev, "qcom,fastchg-current-comp-microamp",\n'
            '\t\t\t\t\t      &comp)) {\n'
            '\t\t\tfor (i = 0; i < ARRAY_SIZE(fcc_comp_ua); i++)\n'
            '\t\t\t\tif (fcc_comp_ua[i] == comp)\n'
            '\t\t\t\t\tbreak;\n'
            '\t\t\tif (i == ARRAY_SIZE(fcc_comp_ua))\n'
            '\t\t\t\treturn dev_err_probe(chip->dev, -EINVAL,\n'
            '\t\t\t\t\t\t     "unsupported fastchg current comp %u\\n", comp);\n'
            '\t\t\tret = qcom_pmic_sec_masked_write(chip->regmap,\n'
            '\t\t\t\t\t\t\t chip->base + SMBCHG_CHGR_FCC_CMP_CFG,\n'
            '\t\t\t\t\t\t\t 0x3, i);\n'
            '\t\t\tif (ret)\n'
            '\t\t\t\treturn ret;\n'
            '\t\t\tdev_info(chip->dev, "soft JEITA charge current %u uA\\n", comp);\n'
            '\t\t}\n'
            '\t}\n'
            '\n'
            '\t/* Set constant charge current limit */\n')
