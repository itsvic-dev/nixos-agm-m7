#define _GNU_SOURCE

#include "virtual-keyboard-unstable-v1-client-protocol.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>

static struct wl_seat *seat;
static struct zwp_virtual_keyboard_manager_v1 *manager;

static void global(void *data, struct wl_registry *registry, uint32_t name, const char *interface,
		   uint32_t version)
{
	if (!strcmp(interface, wl_seat_interface.name) && !seat)
		seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
	else if (!strcmp(interface, zwp_virtual_keyboard_manager_v1_interface.name))
		manager = wl_registry_bind(registry, name, &zwp_virtual_keyboard_manager_v1_interface, 1);
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name)
{
}

static const struct wl_registry_listener listener = { global, global_remove };

static void sleep_ms(int ms)
{
	struct timespec ts = { ms / 1000, (long)(ms % 1000) * 1000000 };

	nanosleep(&ts, NULL);
}

/* Sends evdev key codes. "code:l" holds the key for 800 ms. */
int main(int argc, char **argv)
{
	struct wl_display *display = wl_display_connect(NULL);
	struct xkb_context *ctx = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
	struct xkb_keymap *keymap = xkb_keymap_new_from_names(ctx, NULL, XKB_KEYMAP_COMPILE_NO_FLAGS);
	char *str = xkb_keymap_get_as_string(keymap, XKB_KEYMAP_FORMAT_TEXT_V1);
	size_t size = strlen(str) + 1;
	int fd = memfd_create("keymap", 0);
	struct zwp_virtual_keyboard_v1 *vk;
	uint32_t time = 1;

	if (!display || !keymap || fd < 0 || write(fd, str, size) != (ssize_t)size)
		return 1;
	wl_registry_add_listener(wl_display_get_registry(display), &listener, NULL);
	wl_display_roundtrip(display);
	if (!seat || !manager)
		return 1;

	vk = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(manager, seat);
	zwp_virtual_keyboard_v1_keymap(vk, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, size);
	wl_display_roundtrip(display);

	for (int i = 1; i < argc; i++) {
		uint32_t code = strtoul(argv[i], NULL, 0);
		bool hold = strstr(argv[i], ":l") != NULL;

		zwp_virtual_keyboard_v1_key(vk, time++, code, WL_KEYBOARD_KEY_STATE_PRESSED);
		wl_display_roundtrip(display);
		sleep_ms(hold ? 800 : 30);
		zwp_virtual_keyboard_v1_key(vk, time++, code, WL_KEYBOARD_KEY_STATE_RELEASED);
		wl_display_roundtrip(display);
		sleep_ms(150);
	}
	return 0;
}
