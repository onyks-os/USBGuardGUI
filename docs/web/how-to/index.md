# How-To Guides

Goal-oriented recipes for users who already know the basics. Each guide assumes a working
installation — if you do not have one, start with the [Quickstart](../tutorials/quickstart.md).

!!! note "No guides yet"
    The program is not implemented, and a how-to guide for software nobody can run is fiction. The
    list below is the set of guides the first release will need, in the order they are expected to
    matter. Each becomes a page when the feature it describes exists.

Planned:

- **Install the D-Bus bridge on your distribution** — the single most common first-run obstacle, and
  the one that stops the program working entirely.
- **Grant yourself permission to change USB policy** — what the example Polkit rule does, why it is
  shipped inert rather than installed, and how to decide whether to enable it.
- **Read a diagnostic result** — what each of the nine access states means and which of the three
  checkpoints it points at.
- **Block a device without writing a permanent rule** — the runtime-only versus persisted
  distinction, and why the non-persisting option is the default.
- **Remove a rule safely when something else is editing the policy** — what "the rule is already
  gone" means and why the program refuses rather than guessing.
- **Run it on a desktop with no system tray** — what background mode does instead.
- **Report a bug without leaking your hardware inventory** — which log level to attach, and why.
