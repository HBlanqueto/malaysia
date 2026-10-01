/*
 * A minimal GTK3 window with a plain titlebar: no GtkHeaderBar, so GTK asks
 * the compositor to decorate it through org_kde_kwin_server_decoration.
 *
 * Used by decoration_probe (YUKI_APP_GTK3_PLAIN) to verify the server-side
 * branch of the decoration policy. Build it against the same GTK the probe
 * runs against, for example:
 *
 *   cc gtk3_plain.c -o gtk3_plain $(pkg-config --cflags --libs gtk+-3.0)
 */

#include <gtk/gtk.h>

static void activate(GtkApplication *app, gpointer data)
{
    GtkWidget *window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), "plain");
    gtk_window_set_default_size(GTK_WINDOW(window), 320, 200);
    gtk_widget_show_all(window);
}

int main(int argc, char **argv)
{
    GtkApplication *app = gtk_application_new("org.example.Plain3",
                                              G_APPLICATION_FLAGS_NONE);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
