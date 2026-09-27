// SPDX-License-Identifier: GPL-2.0-only
/*
 * Innolux NT35695 1080x1920 command-mode DSI panel of the Light L16.
 *
 * From the stock L16 device tree (qcom,mdss_dsi_nt35695_innolux_1080p_cmd) and FIH's
 * MDSS changes: the panel's +/-5 V bias comes from two GPIO-switched rails (P5/N5),
 * sequenced around the panel reset; stock never touches the panel regulators
 * (vddio/lab/ibb) on this board.
 *
 * Power-on, as stock: enable high, 1 ms, P5 on, 2 ms, N5 on, 11 ms, then the reset
 * sequence <0 11ms, 1 11ms, 0 1ms, 1 11ms> (reset-gpios is active low here, so the
 * logical values are inverted), then the on-commands in LP mode.
 */

#include <linux/delay.h>
#include <linux/gpio/consumer.h>
#include <linux/module.h>
#include <linux/of.h>

#include <video/mipi_display.h>

#include <drm/drm_connector.h>
#include <drm/drm_mipi_dsi.h>
#include <drm/drm_modes.h>
#include <drm/drm_panel.h>

struct nt35695_l16 {
	struct drm_panel panel;
	struct mipi_dsi_device *dsi;
	struct gpio_desc *enable_gpio;
	struct gpio_desc *p5_gpio;
	struct gpio_desc *n5_gpio;
	struct gpio_desc *reset_gpio;
	enum drm_panel_orientation orientation;
};

static inline struct nt35695_l16 *to_nt35695_l16(struct drm_panel *panel)
{
	return container_of(panel, struct nt35695_l16, panel);
}

static void nt35695_l16_power_on(struct nt35695_l16 *ctx)
{
	gpiod_set_value_cansleep(ctx->enable_gpio, 1);
	usleep_range(1000, 1100);
	gpiod_set_value_cansleep(ctx->p5_gpio, 1);
	usleep_range(2000, 2100);
	gpiod_set_value_cansleep(ctx->n5_gpio, 1);
	usleep_range(11000, 12000);

	/* stock qcom,mdss-dsi-reset-sequence <0 11 1 11 0 1 1 11> (physical levels) */
	gpiod_set_value_cansleep(ctx->reset_gpio, 1);
	usleep_range(11000, 12000);
	gpiod_set_value_cansleep(ctx->reset_gpio, 0);
	usleep_range(11000, 12000);
	gpiod_set_value_cansleep(ctx->reset_gpio, 1);
	usleep_range(1000, 1100);
	gpiod_set_value_cansleep(ctx->reset_gpio, 0);
	usleep_range(11000, 12000);
}

static void nt35695_l16_power_off(struct nt35695_l16 *ctx)
{
	/* stock: P5/N5 off, then reset low; enable stays as MDSS left it */
	gpiod_set_value_cansleep(ctx->p5_gpio, 0);
	gpiod_set_value_cansleep(ctx->n5_gpio, 0);
	gpiod_set_value_cansleep(ctx->reset_gpio, 1);
}

/* stock qcom,mdss-dsi-on-command, sent in LP mode */
static int nt35695_l16_on(struct nt35695_l16 *ctx)
{
	struct mipi_dsi_multi_context dsi_ctx = { .dsi = ctx->dsi };

	ctx->dsi->mode_flags |= MIPI_DSI_MODE_LPM;

	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0xff, 0x23);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0xfb, 0x01);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x07, 0x20);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x08, 0x04);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x46, 0x43);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0xff, 0x10);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x35, 0x00);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x44, 0x05, 0x00);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x51, 0x21);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x53, 0x24);
	mipi_dsi_generic_write_seq_multi(&dsi_ctx, 0x55, 0x00);
	mipi_dsi_dcs_write_seq_multi(&dsi_ctx, MIPI_DCS_EXIT_SLEEP_MODE, 0x00);
	mipi_dsi_msleep(&dsi_ctx, 100);
	mipi_dsi_dcs_write_seq_multi(&dsi_ctx, MIPI_DCS_SET_DISPLAY_ON, 0x00);

	return dsi_ctx.accum_err;
}

/* stock qcom,mdss-dsi-off-command, sent in HS mode */
static void nt35695_l16_off(struct nt35695_l16 *ctx)
{
	struct mipi_dsi_multi_context dsi_ctx = { .dsi = ctx->dsi };

	ctx->dsi->mode_flags &= ~MIPI_DSI_MODE_LPM;

	mipi_dsi_dcs_write_seq_multi(&dsi_ctx, MIPI_DCS_SET_DISPLAY_OFF, 0x00);
	mipi_dsi_msleep(&dsi_ctx, 20);
	mipi_dsi_dcs_write_seq_multi(&dsi_ctx, MIPI_DCS_ENTER_SLEEP_MODE, 0x00);
	mipi_dsi_msleep(&dsi_ctx, 50);
}

static int nt35695_l16_prepare(struct drm_panel *panel)
{
	struct nt35695_l16 *ctx = to_nt35695_l16(panel);
	int ret;

	nt35695_l16_power_on(ctx);

	ret = nt35695_l16_on(ctx);
	if (ret < 0) {
		dev_err(&ctx->dsi->dev, "panel on-commands failed: %d\n", ret);
		nt35695_l16_power_off(ctx);
	}
	return ret;
}

static int nt35695_l16_unprepare(struct drm_panel *panel)
{
	struct nt35695_l16 *ctx = to_nt35695_l16(panel);

	nt35695_l16_off(ctx);
	nt35695_l16_power_off(ctx);
	return 0;
}

/* stock: 1080x1920, h fp/pw/bp 32/8/32, v fp/pw/bp 4/2/9, 60 Hz, 62x110 mm */
static const struct drm_display_mode nt35695_l16_mode = {
	.clock = (1080 + 32 + 8 + 32) * (1920 + 4 + 2 + 9) * 60 / 1000,
	.hdisplay = 1080,
	.hsync_start = 1080 + 32,
	.hsync_end = 1080 + 32 + 8,
	.htotal = 1080 + 32 + 8 + 32,
	.vdisplay = 1920,
	.vsync_start = 1920 + 4,
	.vsync_end = 1920 + 4 + 2,
	.vtotal = 1920 + 4 + 2 + 9,
	.width_mm = 62,
	.height_mm = 110,
};

static int nt35695_l16_get_modes(struct drm_panel *panel, struct drm_connector *connector)
{
	struct drm_display_mode *mode;

	mode = drm_mode_duplicate(connector->dev, &nt35695_l16_mode);
	if (!mode)
		return -ENOMEM;

	drm_mode_set_name(mode);
	mode->type = DRM_MODE_TYPE_DRIVER | DRM_MODE_TYPE_PREFERRED;
	connector->display_info.width_mm = mode->width_mm;
	connector->display_info.height_mm = mode->height_mm;
	drm_mode_probed_add(connector, mode);

	/* the panel is portrait but the L16 is a landscape camera (DT "rotation") */
	drm_connector_set_panel_orientation(connector,
		container_of(panel, struct nt35695_l16, panel)->orientation);

	return 1;
}

static enum drm_panel_orientation nt35695_l16_get_orientation(struct drm_panel *panel)
{
	return container_of(panel, struct nt35695_l16, panel)->orientation;
}

static const struct drm_panel_funcs nt35695_l16_panel_funcs = {
	.prepare = nt35695_l16_prepare,
	.unprepare = nt35695_l16_unprepare,
	.get_modes = nt35695_l16_get_modes,
	.get_orientation = nt35695_l16_get_orientation,
};

static int nt35695_l16_probe(struct mipi_dsi_device *dsi)
{
	struct device *dev = &dsi->dev;
	struct nt35695_l16 *ctx;
	int ret;

	ctx = devm_drm_panel_alloc(dev, struct nt35695_l16, panel,
				   &nt35695_l16_panel_funcs, DRM_MODE_CONNECTOR_DSI);
	if (IS_ERR(ctx))
		return PTR_ERR(ctx);

	/*
	 * The bootloader's splash leaves the panel powered: enable/P5/N5 high and reset
	 * released. Take the pins as outputs at those levels so the panel isn't glitched;
	 * the first prepare runs the full stock power-on sequence.
	 */
	ctx->enable_gpio = devm_gpiod_get(dev, "enable", GPIOD_OUT_HIGH);
	if (IS_ERR(ctx->enable_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->enable_gpio), "enable-gpios\n");
	ctx->p5_gpio = devm_gpiod_get(dev, "p5", GPIOD_OUT_HIGH);
	if (IS_ERR(ctx->p5_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->p5_gpio), "p5-gpios\n");
	ctx->n5_gpio = devm_gpiod_get(dev, "n5", GPIOD_OUT_HIGH);
	if (IS_ERR(ctx->n5_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->n5_gpio), "n5-gpios\n");
	ctx->reset_gpio = devm_gpiod_get(dev, "reset", GPIOD_OUT_LOW);
	if (IS_ERR(ctx->reset_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->reset_gpio), "reset-gpios\n");

	ctx->dsi = dsi;
	mipi_dsi_set_drvdata(dsi, ctx);

	dsi->lanes = 4;
	dsi->format = MIPI_DSI_FMT_RGB888;
	/* command mode (no MIPI_DSI_MODE_VIDEO), stock traffic mode burst */
	dsi->mode_flags = MIPI_DSI_CLOCK_NON_CONTINUOUS;

	ctx->panel.prepare_prev_first = true;

	ret = of_drm_get_panel_orientation(dev->of_node, &ctx->orientation);
	if (ret)
		return dev_err_probe(dev, ret, "Failed to get orientation\n");

	ret = drm_panel_of_backlight(&ctx->panel);
	if (ret)
		return dev_err_probe(dev, ret, "Failed to get backlight\n");

	drm_panel_add(&ctx->panel);

	ret = mipi_dsi_attach(dsi);
	if (ret < 0) {
		dev_err(dev, "Failed to attach to DSI host: %d\n", ret);
		drm_panel_remove(&ctx->panel);
		return ret;
	}

	return 0;
}

static void nt35695_l16_remove(struct mipi_dsi_device *dsi)
{
	struct nt35695_l16 *ctx = mipi_dsi_get_drvdata(dsi);

	mipi_dsi_detach(dsi);
	drm_panel_remove(&ctx->panel);
}

static const struct of_device_id nt35695_l16_of_match[] = {
	{ .compatible = "light,l16-innolux-nt35695" },
	{ }
};
MODULE_DEVICE_TABLE(of, nt35695_l16_of_match);

static struct mipi_dsi_driver nt35695_l16_driver = {
	.probe = nt35695_l16_probe,
	.remove = nt35695_l16_remove,
	.driver = {
		.name = "panel-innolux-nt35695-l16",
		.of_match_table = nt35695_l16_of_match,
	},
};
module_mipi_dsi_driver(nt35695_l16_driver);

MODULE_DESCRIPTION("DRM driver for the Light L16 Innolux NT35695 command-mode DSI panel");
MODULE_LICENSE("GPL v2");
