/*
 * A minimal GTK4 window with a plain titlebar: no GtkHeaderBar, so GTK asks
 * the compositor to decorate it through org_kde_kwin_server_decoration.
 *
 * GTK4 creates that decoration object in realize, before it creates the
 * xdg_toplevel, so this client also exercises the ordering where the mode
 * request arrives before the toplevel exists. Used by decoration_probe
 * (YUKI_APP_GTK4_PLAIN). Build it against the same GTK the probe runs
 * against, for example:
 *
 *   cc gtk4_plain.c -o gtk4_plain $(pkg-config --cflags --libs gtk4-wayland)
 */

#include <gtk/gtk.h>

static void activate(GtkApplication *app, gpointer data)
{
    GtkWidget *window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), "plain");
    gtk_window_set_default_size(GTK_WINDOW(window), 320, 200);
    gtk_widget_set_visible(window, TRUE);
}

int main(int argc, char **argv)
{
    GtkApplication *app = gtk_application_new("org.example.Plain4",
                                              G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
