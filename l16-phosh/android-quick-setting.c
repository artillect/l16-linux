/*
 * A Phosh quick setting for the Light L16's dual boot: "Android" opens a dialog that
 * reboots to the stock system (light-lfc-android-dialog). Only shown when dual booting
 * (light-lfc-bootmode makes /run/light-lfc-dual-boot then).
 *
 * SPDX-License-Identifier: MIT
 */
#include <gtk/gtk.h>
#include <phosh-plugin.h>
#include <quick-setting.h>
#include <status-icon.h>

#define DUAL_BOOT "/run/light-lfc-dual-boot"
#define DIALOG "/usr/libexec/light-lfc-android-dialog"

#define LFC_TYPE_ANDROID_QUICK_SETTING lfc_android_quick_setting_get_type ()
G_DECLARE_FINAL_TYPE (LfcAndroidQuickSetting, lfc_android_quick_setting,
                      LFC, ANDROID_QUICK_SETTING, PhoshQuickSetting)

struct _LfcAndroidQuickSetting {
  PhoshQuickSetting parent;
};

G_DEFINE_TYPE (LfcAndroidQuickSetting, lfc_android_quick_setting, PHOSH_TYPE_QUICK_SETTING)

static void
on_clicked (LfcAndroidQuickSetting *self)
{
  const char *argv[] = { DIALOG, NULL };
  g_autoptr (GError) err = NULL;

  if (!g_spawn_async (NULL, (char **) argv, NULL, G_SPAWN_DEFAULT, NULL, NULL, NULL, &err))
    g_warning ("Can't run %s: %s", DIALOG, err->message);
}

static void
lfc_android_quick_setting_class_init (LfcAndroidQuickSettingClass *klass)
{
}

static void
lfc_android_quick_setting_init (LfcAndroidQuickSetting *self)
{
  PhoshStatusIcon *info = PHOSH_STATUS_ICON (phosh_status_icon_new ());

  phosh_status_icon_set_icon_name (info, "system-reboot-symbolic");
  phosh_status_icon_set_info (info, "Android");
  phosh_status_icon_set_pixel_size (info, 16);
  gtk_widget_set_visible (GTK_WIDGET (info), TRUE);
  phosh_quick_setting_set_status_icon (PHOSH_QUICK_SETTING (self), info);
  g_signal_connect (self, "clicked", G_CALLBACK (on_clicked), NULL);
  gtk_widget_set_visible (GTK_WIDGET (self), g_file_test (DUAL_BOOT, G_FILE_TEST_EXISTS));
}

/* the GIO module */
char **g_io_phosh_plugin_lfc_android_quick_setting_query (void);

void
g_io_module_load (GIOModule *module)
{
  g_type_module_use (G_TYPE_MODULE (module));
  g_io_extension_point_implement (PHOSH_PLUGIN_EXTENSION_POINT_QUICK_SETTING_WIDGET,
                                  LFC_TYPE_ANDROID_QUICK_SETTING,
                                  "lfc-android-quick-setting",
                                  10);
}

void
g_io_module_unload (GIOModule *module)
{
}

char **
g_io_phosh_plugin_lfc_android_quick_setting_query (void)
{
  char *extension_points[] = { PHOSH_PLUGIN_EXTENSION_POINT_QUICK_SETTING_WIDGET, NULL };

  return g_strdupv (extension_points);
}
