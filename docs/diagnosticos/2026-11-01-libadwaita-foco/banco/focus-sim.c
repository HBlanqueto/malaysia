/* Test-only weston module: offers infinity_focus_manager_v1 and, for each
 * infinity_focus_surface_v1, sends the bar events listed in FOCUS_SIM_BARS
 * ("ms:value,ms:value,..." from the object's creation). */
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libweston/libweston.h>
#include "infinity-focus-v1-server-protocol.h"

struct step { int ms; int value; };
struct notice {
  struct wl_resource *resource;
  struct wl_event_source *timer;
  struct step steps[32];
  int n, next, elapsed;
};

static void log_msg (const char *fmt, ...) __attribute__((format(printf,1,2)));
static void log_msg (const char *fmt, ...) {
  va_list ap; va_start (ap, fmt);
  fprintf (stderr, "focus-sim: "); vfprintf (stderr, fmt, ap); fputc ('\n', stderr);
  va_end (ap);
}

static void notice_destroy_req (struct wl_client *c, struct wl_resource *r) { wl_resource_destroy (r); }
static const struct infinity_focus_surface_v1_interface notice_impl = { notice_destroy_req };

static void notice_free (struct wl_resource *r) {
  struct notice *n = wl_resource_get_user_data (r);
  log_msg ("focus surface destroyed");
  wl_event_source_remove (n->timer);
  free (n);
}

static int timer_cb (void *data) {
  struct notice *n = data;
  if (n->next >= n->n) return 0;
  log_msg ("bar(%d) at %d ms", n->steps[n->next].value, n->steps[n->next].ms);
  infinity_focus_surface_v1_send_bar (n->resource, n->steps[n->next].value);
  n->elapsed = n->steps[n->next].ms;
  n->next++;
  if (n->next < n->n)
    wl_event_source_timer_update (n->timer, n->steps[n->next].ms - n->elapsed);
  return 0;
}

static void get_focus_surface (struct wl_client *client, struct wl_resource *manager,
                               uint32_t id, struct wl_resource *surface) {
  struct notice *n = calloc (1, sizeof *n);
  const char *bars = getenv ("FOCUS_SIM_BARS");
  n->resource = wl_resource_create (client, &infinity_focus_surface_v1_interface, 1, id);
  wl_resource_set_implementation (n->resource, &notice_impl, n, notice_free);
  struct wl_event_loop *loop = wl_display_get_event_loop (wl_client_get_display (client));
  n->timer = wl_event_loop_add_timer (loop, timer_cb, n);
  while (bars && *bars && n->n < 32) {
    int ms, v; if (sscanf (bars, "%d:%d", &ms, &v) != 2) break;
    n->steps[n->n++] = (struct step) { ms, v };
    bars = strchr (bars, ','); if (!bars) break; bars++;
  }
  log_msg ("get_focus_surface for wl_surface@%u, %d steps", wl_resource_get_id (surface), n->n);
  if (n->n) wl_event_source_timer_update (n->timer, n->steps[0].ms);
}

static void manager_destroy_req (struct wl_client *c, struct wl_resource *r) { wl_resource_destroy (r); }
static const struct infinity_focus_manager_v1_interface manager_impl = { manager_destroy_req, get_focus_surface };

static void bind_manager (struct wl_client *client, void *data, uint32_t version, uint32_t id) {
  struct wl_resource *r = wl_resource_create (client, &infinity_focus_manager_v1_interface, version, id);
  wl_resource_set_implementation (r, &manager_impl, NULL, NULL);
  log_msg ("manager bound");
}

WL_EXPORT int
wet_module_init (struct weston_compositor *ec, int *argc, char *argv[])
{
  wl_global_create (ec->wl_display, &infinity_focus_manager_v1_interface, 1, NULL, bind_manager);
  log_msg ("global created");
  return 0;
}
