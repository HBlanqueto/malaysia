/* Minimal GTK4 window with a plain titlebar, for decoration_probe. */
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
