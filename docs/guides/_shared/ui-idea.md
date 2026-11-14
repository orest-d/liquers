EDITOR: shared fragment (what the Liquers user interface is for and what it is not, the same in every guide), included at the start of chapter 11 of all three guides. Keep it language-neutral. Written from the owner's brief of 2026-11-14 and reference/UI_INTERFACE_FSD.md.

### What the user interface is, and what it is not

The user interface of Liquers has one goal: that an application built on queries can be shown
on every platform Liquers runs on. Today that is a desktop window, opened from Rust or from
Python, and a page in a browser.

It does not try to be a universal toolkit that hides the platform. There is no Liquers button,
slider or text field that is drawn the same way everywhere and from which you would build any
interface you like. Such a toolkit is a large project of its own, and each platform already has
a better one.

Liquers works at a larger scale instead. Its building blocks are *components*: whole tools that
are useful on their own, such as

- a viewer that shows any value, whether text, a table, an image or an error, together with its
  status and progress while it is computed;
- a console in which you type a query and see its result;
- a view of a Markdown document;
- editors for values, which are being added.

A component on the screen is called an *element*: the tree of elements is what a window or a
page shows.

Each component is written once for each platform, with that platform's own means. So a
component looks and behaves as its platform expects, and it can use everything the platform
offers. What the platforms share is not the drawing but the meaning: which components there
are, which query each one shows, and how they are arranged.

That arrangement is the *UI spec*, and it is the main way to make a user interface. A UI spec
is a small piece of data that assembles components: a menu, a layout, and the queries that fill
it. You do not write code that draws. You say which results should be on the screen and which
queries the menu runs, and each platform draws it with its components. Because the spec is
data and its content is queries, the same spec runs in the window and in the page, and it can
be stored, served and changed like any other file.

| When you want | Use |
|---|---|
| to show results, and let people reach other results from a menu | a UI spec with the components of the library |
| to arrange several views side by side, in tabs or in windows | the layout of a UI spec |
| a button that does something | a menu item whose action is a query; the work is a command |
| a control that does not exist, such as a special chart or form | a component of your own, written with the platform's toolkit; or the platform's toolkit around a Liquers view |

The last row is the boundary. When an application needs a finely tuned interface of its own,
build that interface with the tools of its platform, and use Liquers inside it for what it is
good at: the queries, their results and the components that show them.
