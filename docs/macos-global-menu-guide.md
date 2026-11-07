# The macOS global menu

This is a full transcript of the interactive guide «Global menu — macOS guide»
(Rust + egui), kept in `Craft Anything/revisar`. The text appears as it does in
the app, organized by chapter.

## Navigation

- Global menu — macOS guide
- START: Introduction
- UNDERSTAND: Anatomy, How it works, Behavior
- BUILD: For developers
- LOOK UP: Quick reference
- KEY IDEA: One bar. Every app.
- 6 chapters · macOS 13+

Every page carries the label «INTERACTIVE GUIDE».

---

## 1. Introduction

**Visual guide · macOS**

# The macOS global menu

A single bar at the top of the screen shows the menus of the app in front of
you. This guide explains who owns it, how it changes and what it can do.

> **The bar does not belong to the app**
>
> It belongs to the system. macOS draws it on the top edge of each screen and
> asks the active app which menu to show. When you switch apps, the content
> changes; the bar stays where it is.

### Three ideas to understand it

Remember these three and the rest falls into place.

- **01 · One bar, every app.** There is no bar per window. There is one per
  screen, shared by every app.
- **02 · Focus rules.** The active app decides which titles appear. Switching
  windows changes the menu.
- **03 · Outside the window.** Menus are not drawn inside the window; they live
  on the top edge of the screen.

### A 30-second tour

What happens between clicking an app and choosing an action.

- **01 · You activate an app.** You click its window or pick it with Cmd+Tab.
- **02 · macOS asks for the menu.** The system reads the main menu the app
  declared.
- **03 · The bar updates.** The old titles disappear and the new ones appear.
- **04 · The app responds.** When you choose an action, macOS sends it to the
  active app.

### What it is not

- It is not a taskbar: it shows neither open windows nor apps to launch.
- It is not a menu inside the window: it does not move with the window and you
  cannot dock it.
- It does not make shortcuts global: menu shortcuts work with the active app.

Illustration: a bar with «Notebook», the menus File, Edit, Format, View, Window
and Help, the clock «Wed 9:41», and windows «Notebook — Loose ideas». Captions:
«The bar belongs to the system. The app only supplies its menus.» and «The
window changes; the bar stays.»

---

## 2. Anatomy

**Anatomy · the bar**

# Anatomy of the bar

Five zones, two owners and one rule: the bar looks the same, but its content
changes with the active app.

Illustration labeled «TOP EDGE OF THE SCREEN»: a bar with «Notebook», the menus
File, Edit, Format, View, Window and Help, and the clock «Wed 9:41». Caption:
«The app changes. The bar stays.»

Zones: 1 System, 2 App, 3 Menus, 4 Extras, 5 Clock.

### Zone 1 · System icon

The far left opens actions for the whole Mac: settings, lock, restart and shut
down. It does not depend on the active app.

It is always there.

### Zone 2 · App menu

It carries the app's name on the left side. It gathers the essentials: About,
Settings, Hide and Quit. macOS places it second.

The system paints it; the app defines it.

### Zone 3 · App menus

File, Edit, Format, View… The app decides which ones appear and in what order.
When you switch apps, this whole stretch is replaced.

It is the part that changes most.

### Zone 4 · Status items

The icons on the right. Each one belongs to an app or to an extra that lives in
the bar: Wi-Fi, battery, cloud services, media players…

They are not menus of the active app.

### Zone 5 · Control and clock

Control Center, battery, Wi-Fi and the date. These are system zones. The clock
opens a notifications and calendar view.

Always on the right.

### The menus that are almost always there

The order can vary, but these names repeat because macOS shares conventions
across apps.

- **File:** Create, open, save, export and close.
- **Edit:** Undo, clipboard and selection.
- **Format:** Styles and attributes of the content. Not every app has it.
- **View:** What you see: sidebars, zoom or presentation.
- **Window:** Minimize, zoom and bring windows to the front. macOS treats it as
  special.
- **Help:** Search within the help and shortcuts. It usually includes a search
  field.

### House rules

- The first menu after the system icon carries the app's name.
- Window and Help have standard behavior that macOS expects to find.
- Services appears as a submenu inside the app menu.
- Shortcuts appear right-aligned, apart from the title.
- A disabled item stays visible, in grey: the action exists, but it does not
  apply.

---

## 3. How it works inside

**How it works · inside**

# How it works inside

The system provides the bar; the active app provides the content. This is the
conversation between them.

### The split

- **SYSTEM · Owner of the bar.** It draws the bar, places it, decides when it
  hides and repeats it on every screen.
- **ACTIVE APP · Owner of the menu.** It declares the titles, the actions, the
  shortcuts and which items are available.
- **FOCUS · Owner of the switch.** When another app becomes active, its menu
  replaces the previous one. There are never two bars at once.

### The conversation, step by step

Five moments that repeat every time you switch apps or choose an action.

- **01 · The app starts.** On launch, it registers a main menu: an ordered list
  of menus and items.
- **02 · The system detects focus.** When you click a window, macOS marks that
  app as the active one.
- **03 · The menu is swapped.** The bar takes the titles from the main menu of
  the new active app.
- **04 · You choose an action.** The click travels to the item, which runs a
  method or a system action.
- **05 · The app updates the menu.** Depending on context, it can enable,
  disable or replace items.
- **Always · The bar keeps no state.** It does not remember the previous menu:
  it asks the frontmost app for it again.

### Who decides what

- **Which titles appear:** The active app, through its main menu.
- **Where the bar is:** The system: always on the top edge.
- **Which item is enabled:** The app, through item validation.
- **When it hides:** The system: full screen or automatic hiding.
- **Which shortcut appears:** The app declares it; macOS shows and runs it.

> **Focus changes everything**
>
> The bar does not keep the previous app's menu. As soon as the active app
> changes, the menu is replaced. That is why two apps can have completely
> different menus in the same place.

### Actions, not just titles

- **SHORTCUTS · Key equivalents.** Each item can declare a key. macOS shows it
  on the right and runs it when the app is active.
- **STATE · Validation.** Just before the menu opens, the app can enable or
  disable items depending on context.
- **TARGET · Responder chain.** If an action has no explicit target, macOS
  offers it to the first object that can respond.

Two menus are special: the one that carries the app's name and the Window menu.
macOS expects to find them and gives them standard behavior, such as hiding
the app or bringing windows to the front.

---

## 4. Behavior

**Behavior · simulator**

# How it behaves

Try the simulator: switch apps, open menus and hide the bar. Everything you see
here reproduces the behavior of the global menu.

### Simulator

- Label: SIMULATOR
- Help: «Click a window to change focus; open a menu to see its actions.»
- Toggles: «Simulate full screen», «Hide automatically».
- Button: «Reset».
- Hidden bar: «Move the pointer to the top edge».
- Hint in full screen: «The bar is hidden. Move to the top edge to reveal it,
  just like in macOS.»
- Hint with automatic hiding: «The bar hides by itself. The top edge is still
  the zone that shows it.»
- Normal hint: «When you switch windows, watch how the titles in the bar are
  replaced.»
- When you choose an action: «“{action}” in {app}».

### States worth knowing

The global menu looks static, but it changes with focus, the space and the
screen.

- **App switch.** The titles are replaced instantly. There is no transition and
  no bar per app: the same bar changes its content.
- **Full screen.** The bar hides to give the app all the space. It appears when
  you move the pointer to the top edge.
- **Automatic hiding.** You turn it on in System Settings > Desktop & Dock. The
  bar hides and reveals itself when you approach the top edge.
- **Several screens.** Each screen can have its own bar. With «Displays have
  separate Spaces», each one shows the menus of the active app on that screen.
- **Disabled items.** They stay visible in grey: the action exists, but it is
  not available in that context. This is not the same as hiding it.
- **Submenus and checkmarks.** An arrow on the right marks a submenu. A
  checkmark marks an active option.

> **Something to watch**
>
> Open two apps and switch between them with Cmd+Tab. The bar does not move:
> only the titles between the app name and the icons on the right change.

### Contents of the simulator menus

Apps: **Notebook** (menus File, Edit, Format, View, Window, Help) and
**Gallery** (menus File, Edit, Image, View, Window, Help).

System icon:

- About This Mac
- System Settings…
- ---
- Lock Screen
- Log Out…
- ---
- Restart…
- Shut Down…

App menu (Notebook):

- About Notebook
- ---
- Settings… (Cmd ,)
- ---
- Hide Notebook (Cmd H)
- Hide Others (Cmd Opt H)
- ---
- Quit Notebook (Cmd Q)

File (Notebook):

- New Note (Cmd N)
- Open… (Cmd O)
- ---
- Save (Cmd S)
- Export › (submenu)
- ---
- Close Window (Cmd W) — disabled

File (Gallery):

- Import… (Cmd I)
- Export…
- ---
- Save a Copy (Cmd S)
- ---
- Close (Cmd W)

Edit (both apps):

- Undo (Cmd Z)
- Redo (Cmd Shift Z) — disabled
- ---
- Cut (Cmd X)
- Copy (Cmd C)
- Paste (Cmd V)
- ---
- Select All (Cmd A)

Format (Notebook):

- Heading
- List
- ---
- Bold (Cmd B)
- Italic (Cmd I)

Image (Gallery):

- Rotate Left
- Rotate Right
- ---
- Crop
- Color Adjustments › (submenu)

View (Notebook):

- Show Sidebar
- ---
- Zoom › (submenu)
- Full Screen (Ctrl Cmd F)

View (Gallery):

- Thumbnails
- Grid
- ---
- Full Screen (Ctrl Cmd F)

Window (both apps):

- Minimize (Cmd M)
- Zoom
- ---
- Bring All to Front

Help (both apps):

- Search Help
- ---
- Keyboard Shortcuts

---

## 5. For developers

**For developers**

# Where the menu is defined

The app does not draw the bar: it declares a menu. macOS decides where to show
it and what to do with the actions.

Tabs: AppKit, SwiftUI, Menu bar extra.

### Swift · AppKit

```swift
import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ note: Notification) {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        appItem.submenu = NSMenu(title: "Notebook")
        mainMenu.addItem(appItem)

        let fileItem = NSMenuItem()
        fileItem.submenu = NSMenu(title: "File")
        fileItem.submenu?.addItem(
            withTitle: "New Note",
            action: #selector(newNote),
            keyEquivalent: "n"
        )
        mainMenu.addItem(fileItem)

        NSApp.mainMenu = mainMenu
    }

    @objc func newNote() {
        // open a new note
    }
}
```

### Swift · SwiftUI

```swift
import SwiftUI

@main
struct NotebookApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .commands {
            CommandMenu("Notas") {
                Button("New Note") { newNote() }
                    .keyboardShortcut("n")
                Divider()
                Button("Exportar…") { exportar() }
            }
        }
    }
}
```

### Swift · MenuBarExtra

```swift
import SwiftUI
import AppKit

@main
struct NotebookApp: App {
    var body: some Scene {
        MenuBarExtra("Notebook", systemImage: "square.and.pencil") {
            Button("New Note") { newNote() }
            Button("Ajustes…") { abrirAjustes() }
            Divider()
            Button("Salir") {
                NSApplication.shared.terminate(nil)
            }
        }
        .menuBarExtraStyle(.menu)
    }
}
```

The example uses neutral names and leaves out localization. In a real app, the
titles are translated and the standard items are built with the AppKit APIs.

### Where each thing lives

- `NSApplication.mainMenu` — The root menu of the active app.
- `NSMenu` — A menu with a title; it holds items and separators.
- `NSMenuItem` — An action, with a shortcut, a state and an optional submenu.
- `NSStatusItem` — A custom icon in the right zone of the bar.
- `MenuBarExtra` — The SwiftUI equivalent for a menu bar extra.
- `validateMenuItem` — Decides whether an item can be chosen in this context.

### Order matters

- The first item of the root menu is the app menu; macOS sets its title.
- Next come the menus the app declares: File, Edit and its own.
- Each item can have an action, a shortcut and a submenu.
- Extras live in the right zone, but they are not the active app's menu.
- Validation runs when the menu opens; there you can change the state of each
  item.

> **There is no API to embed the global bar**
>
> The bar is part of the system. If a cross-platform app draws menus inside the
> window, it imitates the pattern; it does not use the macOS global bar.

The global bar is not the same as a global shortcut. Menu shortcuts work while
the app is active; shortcuts that work from any app use other system APIs.

---

## 6. Reference and questions

**Quick reference**

# Reference and questions

The terms, the shortcuts and the questions that come up when you work with the
global menu.

### Glossary

- **Active app.** The app that has focus. It supplies the menu you see in the
  bar.
- **Main menu.** The root list of menus that the app declares on launch. macOS
  turns it into the bar.
- **App menu.** The menu with the app's name: About, Settings, Hide and Quit.
- **Menu bar extra.** A custom icon in the right zone. It is not the active
  app's menu.
- **Responder.** The object that receives an action when the action has no
  explicit target.
- **Validation.** The check that decides whether a menu item is available.

### Frequently asked questions

**Why does my app not show custom menus?**
Most often the main menu was not assigned, or it was created after launch
finished. In AppKit, assign NSApp.mainMenu once the menu is built; in SwiftUI,
add the .commands modifier to the scene.

**Can I move the bar inside the window?**
No. The global bar is part of the system and cannot be embedded in a window.
You can draw a menu inside your window, but that is another pattern and it does
not get the global menu's behavior.

**How do I add an icon on the right?**
With an NSStatusItem in AppKit or a MenuBarExtra in SwiftUI. The icon can open
its own menu; it stays in the bar even when the active app changes.

**Why does it disappear in full screen?**
macOS hides the bar to give more space. It reveals itself when you move the
pointer to the top edge. The desktop's automatic hiding works in a similar way.

**Do shortcuts work when the app is not active?**
No. Shortcuts declared in the menu belong to the active app. Shortcuts that
work from any app need global shortcut APIs, which are a different thing.

**What happens with several screens?**
Each screen can show its own bar. With «Displays have separate Spaces», each
screen has its own active space and its own active app; without it, the main
bar rules.

### Shortcuts you will see in menus

- Cmd , — App settings
- Cmd H — Hide the app
- Cmd Q — Quit the app
- Cmd W — Close the window
- Cmd M — Minimize the window
- Ctrl Cmd F — Full screen

### Further reading

Apple documentation: NSMenu, NSMenuItem, NSStatusItem and MenuBarExtra. Look
for the «Menu bar» and «Status bar» guides in the developer documentation.

---

End of the guide.
