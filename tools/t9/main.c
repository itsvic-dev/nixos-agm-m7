#define _GNU_SOURCE

#include "engine.h"
#include "input-method-unstable-v2-client-protocol.h"
#include "virtual-keyboard-unstable-v1-client-protocol.h"

#include <cairo.h>
#include <errno.h>
#include <linux/input-event-codes.h>
#include <math.h>
#include <pango/pangocairo.h>
#include <poll.h>
#include <signal.h>
#include <spawn.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/timerfd.h>
#include <unistd.h>
#include <wayland-client.h>

#ifndef T9_DATADIR
#define T9_DATADIR "."
#endif
#ifndef SWAYMSG
#define SWAYMSG "swaymsg"
#endif
#ifndef PKILL
#define PKILL "pkill"
#endif

#define LONG_PRESS_MS 500
#define TAP_TIMEOUT_MS 1000
#define MODE_SHOW_MS 1500
#define POPUP_ROWS 5
#define FONT "Inter Medium 8.5"

extern char **environ;

struct state {
	struct wl_display *display;
	struct wl_compositor *compositor;
	struct wl_shm *shm;
	struct wl_seat *seat;
	struct zwp_input_method_manager_v2 *im_manager;
	struct zwp_virtual_keyboard_manager_v1 *vk_manager;

	struct zwp_input_method_v2 *im;
	struct zwp_input_method_keyboard_grab_v2 *grab;
	struct zwp_virtual_keyboard_v1 *vk;
	struct wl_surface *popup_surface;
	struct zwp_input_popup_surface_v2 *popup;

	uint32_t serial;
	bool pending_active, active;
	bool keymap_set;
	char *surrounding;
	uint32_t surrounding_cursor;
	bool surrounding_changed;

	struct t9 t9;
	struct t9_dict dicts[8];

	int long_fd, tap_fd, mode_fd;
	uint32_t held_code;
	int held_key;
	bool long_fired;
	bool show_mode;
	bool forwarded[KEY_CNT];
	uint32_t time;
};

static void spawn(char *const argv[])
{
	pid_t pid;

	posix_spawnp(&pid, argv[0], NULL, NULL, argv, environ);
}

static void sway_mode(const char *mode)
{
	char *argv[] = { SWAYMSG, "mode", (char *)mode, NULL };

	spawn(argv);
}

static void arm(int fd, int ms)
{
	struct itimerspec ts = {
		.it_value = { .tv_sec = ms / 1000, .tv_nsec = (long)(ms % 1000) * 1000000 },
	};

	timerfd_settime(fd, 0, &ts, NULL);
}

static void disarm(int fd)
{
	arm(fd, 0);
}

static int key_of_code(uint32_t code)
{
	switch (code) {
	case KEY_0:
		return 0;
	case KEY_1 ... KEY_9:
		return code - KEY_1 + 1;
	case KEY_NUMERIC_STAR:
		return T9_KEY_STAR;
	case KEY_NUMERIC_POUND:
		return T9_KEY_POUND;
	case KEY_BACK:
		return T9_KEY_BACK;
	case KEY_UP:
		return T9_KEY_UP;
	case KEY_DOWN:
		return T9_KEY_DOWN;
	}
	return -1;
}

static void buffer_release(void *data, struct wl_buffer *buffer)
{
	wl_buffer_destroy(buffer);
}

static const struct wl_buffer_listener buffer_listener = {
	.release = buffer_release,
};

static void draw_text(cairo_t *cr, PangoLayout *layout, const char *text, double x, double y, uint32_t rgb)
{
	cairo_set_source_rgb(cr, (rgb >> 16) / 255.0, ((rgb >> 8) & 0xff) / 255.0, (rgb & 0xff) / 255.0);
	cairo_move_to(cr, x, y);
	pango_layout_set_text(layout, text, -1);
	pango_cairo_show_layout(cr, layout);
}

static void rounded(cairo_t *c, double x, double y, double w, double h, double r)
{
	cairo_new_sub_path(c);
	cairo_arc(c, x + w - r, y + r, r, -M_PI / 2, 0);
	cairo_arc(c, x + w - r, y + h - r, r, 0, M_PI / 2);
	cairo_arc(c, x + r, y + h - r, r, M_PI / 2, M_PI);
	cairo_arc(c, x + r, y + r, r, M_PI, 3 * M_PI / 2);
	cairo_close_path(c);
}

static void draw_popup(struct state *st)
{
	const struct t9 *t = &st->t9;
	cairo_surface_t *measure = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 1, 1);
	cairo_t *cr = cairo_create(measure);
	PangoLayout *layout = pango_cairo_create_layout(cr);
	PangoFontDescription *font = pango_font_description_from_string(FONT);
	bool composing = t9_composing(t);
	int first = 0, rows = 0, line_h, width = 0, height, stride, w, h, fd;
	char header[32];
	struct wl_shm_pool *pool;
	struct wl_buffer *buffer;
	void *data;

	if (!composing && !st->show_mode) {
		wl_surface_attach(st->popup_surface, NULL, 0, 0);
		wl_surface_commit(st->popup_surface);
		goto out;
	}

	pango_layout_set_font_description(layout, font);
	t9_mode_name(t, header, sizeof(header));
	pango_layout_set_text(layout, header, -1);
	pango_layout_get_pixel_size(layout, &width, &line_h);

	if (composing) {
		rows = t->ncand < POPUP_ROWS ? t->ncand : POPUP_ROWS;
		first = t->selected - POPUP_ROWS / 2;
		if (first > t->ncand - rows)
			first = t->ncand - rows;
		if (first < 0)
			first = 0;
		for (int i = first; i < first + rows; i++) {
			pango_layout_set_text(layout, t->cand[i], -1);
			pango_layout_get_pixel_size(layout, &w, &h);
			if (w > width)
				width = w;
		}
	}
	line_h += 4;
	width += 16;
	height = (rows + 1) * line_h + 6;
	stride = cairo_format_stride_for_width(CAIRO_FORMAT_ARGB32, width);

	fd = memfd_create("t9-popup", MFD_CLOEXEC);
	if (fd < 0 || ftruncate(fd, (off_t)stride * height) < 0)
		goto out_fd;
	data = mmap(NULL, (size_t)stride * height, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (data == MAP_FAILED)
		goto out_fd;

	{
		cairo_surface_t *surface =
			cairo_image_surface_create_for_data(data, CAIRO_FORMAT_ARGB32, width, height, stride);
		cairo_t *c = cairo_create(surface);

		pango_cairo_update_layout(c, layout);
		cairo_set_operator(c, CAIRO_OPERATOR_CLEAR);
		cairo_paint(c);
		cairo_set_operator(c, CAIRO_OPERATOR_OVER);
		rounded(c, 0.5, 0.5, width - 1, height - 1, 7);
		cairo_set_source_rgb(c, 1, 1, 1);
		cairo_fill_preserve(c);
		cairo_set_source_rgb(c, 0xd6 / 255.0, 0xd6 / 255.0, 0xdc / 255.0);
		cairo_set_line_width(c, 1);
		cairo_stroke(c);

		draw_text(c, layout, header, 8, 4, 0x6b6b75);
		for (int i = 0; i < rows; i++) {
			int y = 3 + (i + 1) * line_h;
			bool selected = first + i == t->selected;

			if (selected) {
				rounded(c, 3, y, width - 6, line_h, 5);
				cairo_set_source_rgb(c, 0xe6 / 255.0, 0xef / 255.0, 1);
				cairo_fill(c);
			}
			draw_text(c, layout, t->cand[first + i], 8, y + 2, selected ? 0x2f6fec : 0x16161a);
		}
		cairo_destroy(c);
		cairo_surface_flush(surface);
		cairo_surface_destroy(surface);
	}
	munmap(data, (size_t)stride * height);

	pool = wl_shm_create_pool(st->shm, fd, stride * height);
	buffer = wl_shm_pool_create_buffer(pool, 0, width, height, stride, WL_SHM_FORMAT_ARGB8888);
	wl_shm_pool_destroy(pool);
	wl_buffer_add_listener(buffer, &buffer_listener, NULL);
	wl_surface_attach(st->popup_surface, buffer, 0, 0);
	wl_surface_damage_buffer(st->popup_surface, 0, 0, width, height);
	wl_surface_commit(st->popup_surface);

out_fd:
	if (fd >= 0)
		close(fd);
out:
	pango_font_description_free(font);
	g_object_unref(layout);
	cairo_destroy(cr);
	cairo_surface_destroy(measure);
}

static void send_key(struct state *st, uint32_t code, uint32_t state)
{
	if (st->keymap_set)
		zwp_virtual_keyboard_v1_key(st->vk, st->time, code, state);
}

static void apply(struct state *st, const struct t9_out *out)
{
	const char *preedit = t9_preedit(&st->t9);

	if (out->commit[0])
		zwp_input_method_v2_commit_string(st->im, out->commit);
	zwp_input_method_v2_set_preedit_string(st->im, preedit, strlen(preedit), strlen(preedit));
	zwp_input_method_v2_commit(st->im, st->serial);

	for (int i = 0; i < out->backspaces; i++) {
		send_key(st, KEY_BACKSPACE, WL_KEYBOARD_KEY_STATE_PRESSED);
		send_key(st, KEY_BACKSPACE, WL_KEYBOARD_KEY_STATE_RELEASED);
	}
	if (out->toggle_osk) {
		char *argv[] = { PKILL, "-RTMIN", "wvkbd-mobintl", NULL };

		spawn(argv);
	}
	if (out->mode_changed) {
		st->show_mode = true;
		arm(st->mode_fd, MODE_SHOW_MS);
	}
	if (t9_tap_pending(&st->t9))
		arm(st->tap_fd, TAP_TIMEOUT_MS);
	else
		disarm(st->tap_fd);
	draw_popup(st);
}

static void flush(struct state *st)
{
	struct t9_out out = { 0 };

	if (!t9_composing(&st->t9))
		return;
	t9_flush(&st->t9, &out);
	apply(st, &out);
}

static void finish_held(struct state *st, bool long_press)
{
	struct t9_out out;

	disarm(st->long_fd);
	t9_key(&st->t9, st->held_key, long_press, &out);
	apply(st, &out);
}

static void grab_keymap(void *data, struct zwp_input_method_keyboard_grab_v2 *grab, uint32_t format, int32_t fd,
			uint32_t size)
{
	struct state *st = data;

	if (format == WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1 && size) {
		zwp_virtual_keyboard_v1_keymap(st->vk, format, fd, size);
		st->keymap_set = true;
	}
	close(fd);
}

static void grab_key(void *data, struct zwp_input_method_keyboard_grab_v2 *grab, uint32_t serial, uint32_t time,
		     uint32_t code, uint32_t state)
{
	struct state *st = data;
	int key = key_of_code(code);
	struct t9_out out;

	st->time = time;
	if (code >= KEY_CNT)
		return;

	if (state == WL_KEYBOARD_KEY_STATE_RELEASED) {
		if (st->forwarded[code]) {
			st->forwarded[code] = false;
			send_key(st, code, state);
		} else if (code == st->held_code) {
			if (!st->long_fired)
				finish_held(st, false);
			st->held_code = 0;
		}
		return;
	}

	if (st->held_code) {
		if (!st->long_fired)
			finish_held(st, false);
		st->held_code = 0;
	}

	if (key == T9_KEY_UP || key == T9_KEY_DOWN) {
		t9_key(&st->t9, key, false, &out);
		apply(st, &out);
		if (out.forward) {
			st->forwarded[code] = true;
			send_key(st, code, state);
		}
	} else if (key >= 0) {
		st->held_code = code;
		st->held_key = key;
		st->long_fired = false;
		arm(st->long_fd, LONG_PRESS_MS);
	} else {
		flush(st);
		st->forwarded[code] = true;
		send_key(st, code, state);
	}
}

static void grab_modifiers(void *data, struct zwp_input_method_keyboard_grab_v2 *grab, uint32_t serial,
			   uint32_t depressed, uint32_t latched, uint32_t locked, uint32_t group)
{
	struct state *st = data;

	if (st->keymap_set)
		zwp_virtual_keyboard_v1_modifiers(st->vk, depressed, latched, locked, group);
}

static void grab_repeat_info(void *data, struct zwp_input_method_keyboard_grab_v2 *grab, int32_t rate,
			     int32_t delay)
{
}

static const struct zwp_input_method_keyboard_grab_v2_listener grab_listener = {
	.keymap = grab_keymap,
	.key = grab_key,
	.modifiers = grab_modifiers,
	.repeat_info = grab_repeat_info,
};

static void im_activate(void *data, struct zwp_input_method_v2 *im)
{
	struct state *st = data;

	st->pending_active = true;
	free(st->surrounding);
	st->surrounding = NULL;
	st->surrounding_changed = false;
}

static void im_deactivate(void *data, struct zwp_input_method_v2 *im)
{
	struct state *st = data;

	st->pending_active = false;
}

static void im_surrounding_text(void *data, struct zwp_input_method_v2 *im, const char *text, uint32_t cursor,
				uint32_t anchor)
{
	struct state *st = data;

	free(st->surrounding);
	st->surrounding = strdup(text);
	st->surrounding_cursor = cursor;
	st->surrounding_changed = true;
}

static void im_text_change_cause(void *data, struct zwp_input_method_v2 *im, uint32_t cause)
{
}

static void im_content_type(void *data, struct zwp_input_method_v2 *im, uint32_t hint, uint32_t purpose)
{
}

static bool at_sentence_start(const char *text, uint32_t cursor)
{
	size_t len = strlen(text);

	if (cursor < len)
		len = cursor;
	while (len && text[len - 1] == ' ')
		len--;
	return !len || strchr(".?!\n", text[len - 1]);
}

static void im_done(void *data, struct zwp_input_method_v2 *im)
{
	struct state *st = data;

	st->serial++;
	if (st->pending_active != st->active) {
		st->active = st->pending_active;
		t9_reset(&st->t9);
		disarm(st->long_fd);
		disarm(st->tap_fd);
		disarm(st->mode_fd);
		st->held_code = 0;
		st->show_mode = false;
		memset(st->forwarded, 0, sizeof(st->forwarded));
		if (st->active) {
			t9_set_sentence_start(&st->t9, true);
			st->grab = zwp_input_method_v2_grab_keyboard(st->im);
			zwp_input_method_keyboard_grab_v2_add_listener(st->grab, &grab_listener, st);
			sway_mode("text");
		} else {
			zwp_input_method_keyboard_grab_v2_release(st->grab);
			st->grab = NULL;
			sway_mode("default");
		}
		draw_popup(st);
	}
	if (st->active && st->surrounding_changed && !t9_composing(&st->t9))
		t9_set_sentence_start(&st->t9, at_sentence_start(st->surrounding, st->surrounding_cursor));
	st->surrounding_changed = false;
}

static void im_unavailable(void *data, struct zwp_input_method_v2 *im)
{
	fprintf(stderr, "t9: another input method is running\n");
	exit(1);
}

static const struct zwp_input_method_v2_listener im_listener = {
	.activate = im_activate,
	.deactivate = im_deactivate,
	.surrounding_text = im_surrounding_text,
	.text_change_cause = im_text_change_cause,
	.content_type = im_content_type,
	.done = im_done,
	.unavailable = im_unavailable,
};

static void popup_rectangle(void *data, struct zwp_input_popup_surface_v2 *popup, int32_t x, int32_t y,
			    int32_t width, int32_t height)
{
}

static const struct zwp_input_popup_surface_v2_listener popup_listener = {
	.text_input_rectangle = popup_rectangle,
};

static void registry_global(void *data, struct wl_registry *registry, uint32_t name, const char *interface,
			    uint32_t version)
{
	struct state *st = data;

	if (!strcmp(interface, wl_compositor_interface.name))
		st->compositor = wl_registry_bind(registry, name, &wl_compositor_interface, 4);
	else if (!strcmp(interface, wl_shm_interface.name))
		st->shm = wl_registry_bind(registry, name, &wl_shm_interface, 1);
	else if (!strcmp(interface, wl_seat_interface.name) && !st->seat)
		st->seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
	else if (!strcmp(interface, zwp_input_method_manager_v2_interface.name))
		st->im_manager = wl_registry_bind(registry, name, &zwp_input_method_manager_v2_interface, 1);
	else if (!strcmp(interface, zwp_virtual_keyboard_manager_v1_interface.name))
		st->vk_manager = wl_registry_bind(registry, name, &zwp_virtual_keyboard_manager_v1_interface, 1);
}

static void registry_global_remove(void *data, struct wl_registry *registry, uint32_t name)
{
}

static const struct wl_registry_listener registry_listener = {
	.global = registry_global,
	.global_remove = registry_global_remove,
};

static void load_dicts(struct state *st)
{
	const char *dir = getenv("T9_DATA");

	if (!dir)
		dir = T9_DATADIR;
	for (int i = 0; i < t9_lang_count; i++) {
		char path[4096];
		FILE *f;

		snprintf(path, sizeof(path), "%s/%s", dir, t9_langs[i].file);
		f = fopen(path, "r");
		if (!f) {
			fprintf(stderr, "t9: %s: %s\n", path, strerror(errno));
			continue;
		}
		if (t9_dict_load(&st->dicts[i], &t9_langs[i], f))
			fprintf(stderr, "t9: %s: out of memory\n", path);
		fclose(f);
	}
}

static bool timer_fired(struct pollfd *pfd)
{
	uint64_t n;

	return (pfd->revents & POLLIN) && read(pfd->fd, &n, sizeof(n)) == sizeof(n);
}

int main(int argc, char **argv)
{
	static struct state st;
	struct wl_registry *registry;
	int lang = 0;

	signal(SIGCHLD, SIG_IGN);

	for (int i = 0; argc > 1 && i < t9_lang_count; i++) {
		if (!strcasecmp(argv[1], t9_langs[i].name))
			lang = i;
	}
	load_dicts(&st);
	t9_init(&st.t9, st.dicts, lang);

	st.display = wl_display_connect(NULL);
	if (!st.display) {
		fprintf(stderr, "t9: cannot connect to the Wayland display\n");
		return 1;
	}
	registry = wl_display_get_registry(st.display);
	wl_registry_add_listener(registry, &registry_listener, &st);
	wl_display_roundtrip(st.display);
	if (!st.compositor || !st.shm || !st.seat || !st.im_manager || !st.vk_manager) {
		fprintf(stderr, "t9: the compositor does not support input-method-v2 and virtual-keyboard-v1\n");
		return 1;
	}

	st.im = zwp_input_method_manager_v2_get_input_method(st.im_manager, st.seat);
	zwp_input_method_v2_add_listener(st.im, &im_listener, &st);
	st.vk = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(st.vk_manager, st.seat);
	st.popup_surface = wl_compositor_create_surface(st.compositor);
	st.popup = zwp_input_method_v2_get_input_popup_surface(st.im, st.popup_surface);
	zwp_input_popup_surface_v2_add_listener(st.popup, &popup_listener, &st);

	st.long_fd = timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC | TFD_NONBLOCK);
	st.tap_fd = timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC | TFD_NONBLOCK);
	st.mode_fd = timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC | TFD_NONBLOCK);

	for (;;) {
		struct pollfd fds[] = {
			{ .fd = wl_display_get_fd(st.display), .events = POLLIN },
			{ .fd = st.long_fd, .events = POLLIN },
			{ .fd = st.tap_fd, .events = POLLIN },
			{ .fd = st.mode_fd, .events = POLLIN },
		};

		while (wl_display_prepare_read(st.display))
			wl_display_dispatch_pending(st.display);
		if (wl_display_flush(st.display) < 0 && errno != EAGAIN)
			break;
		if (poll(fds, 4, -1) < 0) {
			wl_display_cancel_read(st.display);
			if (errno == EINTR)
				continue;
			break;
		}
		if (fds[0].revents & POLLIN) {
			if (wl_display_read_events(st.display) < 0)
				break;
		} else {
			wl_display_cancel_read(st.display);
		}
		wl_display_dispatch_pending(st.display);

		if (timer_fired(&fds[1]) && st.held_code && !st.long_fired && st.active) {
			st.long_fired = true;
			finish_held(&st, true);
		}
		if (timer_fired(&fds[2]) && st.active) {
			struct t9_out out;

			t9_timeout(&st.t9, &out);
			apply(&st, &out);
		}
		if (timer_fired(&fds[3])) {
			st.show_mode = false;
			draw_popup(&st);
		}
		if (fds[0].revents & (POLLERR | POLLHUP))
			break;
	}
	return 0;
}
