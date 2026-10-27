// Magazine 6's demonstrations of the library's controls (605 to 612): check
// boxes and radios, switches and sliders, numbers and selects, text fields,
// suggestions, a form that hears its fields' events, dialogs over a page,
// and a wizard. Each in its variants: with labels and without, disabled,
// in colours, with sounds.

import { Colour } from "../kit.ts";
import {
  Autocomplete,
  Button,
  Checkbox,
  CheckboxGroup,
  type Child,
  Columns,
  Component,
  type ComponentEvent,
  Dialog,
  type Element,
  Field,
  Gap,
  h,
  type HoverEvent,
  Modal,
  NumberInput,
  type PageSpec,
  Panel,
  Paragraph,
  RadioGroup,
  Rows,
  Rule,
  Select,
  Slider,
  Spinner,
  Switch,
  TextField,
  ToastShelf,
  Wizard,
} from "../../../../kits/typescript-components/src/index.ts";
import { SOUNDS } from "../assets.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const BOXES = 0x605;
export const SWITCHES = 0x606;
export const NUMBERS = 0x607;
export const FIELDS = 0x608;
export const SUGGEST = 0x609;
export const FORM = 0x610;
export const DIALOGS = 0x611;
export const WIZARD = 0x612;

const SOUNDED = { move: SOUNDS.tick, change: SOUNDS.tick, choose: SOUNDS.chime, error: SOUNDS.wrong } as const;

const COUNTRIES = ["Australia", "Austria", "Belgium", "Brazil", "Canada", "Chile", "Denmark", "Finland", "France", "Germany", "Iceland", "India", "Ireland", "Italy", "Japan", "Kenya", "Norway", "Peru", "Spain", "Sweden"];

/** Listens to every control under it, and says what the last one did: events going up the tree, and the pointer coming onto a control and going off it. */
class Heard extends Component<object, { readonly last: string }> {
  override state = { last: "Change a control: its event comes up to here" };

  override onComponentEvent(event: ComponentEvent): boolean {
    const value = "value" in event ? JSON.stringify(event.value) : "";
    this.setState({ last: `${event.type} ${value} (${event.origin})`.slice(0, 60) });
    return false;
  }

  override onHover(event: HoverEvent): boolean {
    this.setState({ last: `Pointer ${event.entered ? "on" : "off"} the control at ${event.row},${event.column}` });
    return false;
  }

  override render(): Child {
    return h(Rows, {}, ...(this.props.children ?? []), h(Gap, { rows: 0, grow: 1 }), h(Paragraph, { text: this.state.last, foreground: Colour.Cyan, lines: 1 }));
  }
}

class FormDemo extends Component<object, { readonly name: string; readonly email: string; readonly plan: number; readonly news: boolean; readonly sent: string }> {
  override state = { name: "", email: "", plan: 0, news: false, sent: "" };

  override render(): Child {
    const ready = this.state.name.trim() !== "" && /@.+\./u.test(this.state.email);
    return [
      h(
        Panel,
        { title: "Your details", foreground: Colour.Cyan },
        h(Field, { label: "Name" }, h(TextField, { longest: 20, value: this.state.name, placeholder: "your name", onChange: (name: string) => this.setState({ name }) })),
        h(Field, { label: "Email" }, h(TextField, { longest: 24, value: this.state.email, validate: (email: string) => (/@.+\./u.test(email) ? null : "not an address yet"), onChange: (email: string) => this.setState({ email }) })),
      ),
      h(
        Panel,
        { title: "Your plan", foreground: Colour.Green },
        h(Field, { label: "Plan" }, h(RadioGroup, { items: ["Free", "Club", "Patron"], across: true, value: this.state.plan, onChange: (plan: number) => this.setState({ plan }) })),
        h(Field, { label: "" }, h(Checkbox, { label: "Send me the newsletter", value: this.state.news, onChange: (news) => this.setState({ news: news === true }) })),
      ),
      h(Button, {
        label: ready ? "Send" : "Send (fill in both)",
        disabled: !ready,
        onPress: (session: number) => {
          this.setState({ sent: `Sent for ${this.state.name}` });
          this.services.bus.emit("toast", { kind: "success", text: `Thank you, ${this.state.name}` }, { session });
        },
      }),
      h(Paragraph, { text: this.state.sent, foreground: Colour.Green }),
      h(ToastShelf, { maxShown: 1 }),
    ];
  }
}

type DialogKind = "none" | "alert" | "confirm" | "prompt" | "busy";

class DialogDemo extends Component<object, { readonly open: DialogKind; readonly said: string; readonly typed: string }> {
  override state = { open: "none" as DialogKind, said: "Nothing answered yet", typed: "" };

  #close(said: string): void {
    this.setState({ open: "none", said });
  }

  #dialog(): Element | null {
    switch (this.state.open) {
      case "alert":
        return h(Dialog, { title: "Saved", kind: "success", message: "The page was saved.", buttons: ["OK"], onAnswer: () => this.#close("OK") });
      case "confirm":
        return h(Dialog, { title: "Delete page 304?", kind: "warning", message: "It cannot be had back.", buttons: ["Delete", "Keep it"], initial: 1, onAnswer: (button: number | null) => this.#close(button === 0 ? "Deleted" : button === 1 ? "Kept" : "Escaped") });
      case "prompt":
        return h(
          Dialog,
          { title: "Rename the page", message: "Type its new name, then Enter.", buttons: ["Rename", "Cancel"], childFirst: true, onAnswer: (button: number | null) => this.#close(button === 0 ? `Renamed to "${this.state.typed}"` : "Not renamed") },
          h(TextField, { longest: 20, autoFocus: true, value: this.state.typed, onChange: (typed: string) => this.setState({ typed }) }),
        );
      case "busy":
        return h(Dialog, { title: "Please wait", kind: "info", message: "Working on it; it closes itself.", onAnswer: () => this.#close("Escaped the wait") }, h(Spinner, { label: "Working", style: "orbit" }));
      case "none":
        return null;
    }
  }

  override render(): Child {
    const open = (kind: DialogKind) => (): void => {
      this.setState({ open: kind });
      if (kind === "busy") {
        this.setTimeout(() => this.#close("The work finished"), 4_000);
      }
    };
    return h(
      Modal,
      { dialog: this.#dialog(), row: 4 },
      h(Paragraph, { text: "Each button opens a dialog over the page. It keeps the keys until it is answered.", foreground: Colour.White }),
      h(Gap, {}),
      h(Columns, { height: 1 }, h(Button, { label: "Alert", onPress: open("alert") }), h(Button, { label: "Confirm", background: Colour.White, onPress: open("confirm") })),
      h(Gap, {}),
      h(Columns, { height: 1 }, h(Button, { label: "Prompt", background: Colour.Green, onPress: open("prompt") }), h(Button, { label: "Busy", background: Colour.Magenta, onPress: open("busy") })),
      h(Gap, {}),
      h(Paragraph, { text: `Answer: ${this.state.said}`, foreground: Colour.Yellow }),
      h(Gap, { rows: 6 }),
      h(Paragraph, { text: "Under the dialog the page goes on: this line is hidden while one is open.", foreground: Colour.Blue }),
    );
  }
}

class WizardDemo extends Component<object, { readonly name: string; readonly size: number; readonly done: boolean }> {
  override state = { name: "", size: 1, done: false };

  override render(): Child {
    return h(
      Wizard,
      {
        steps: ["Name", "Size", "Check"],
        ready: (step: number) => (step === 0 && this.state.name.trim() === "" ? "Type a name first (Tab to the field)" : null),
        onFinish: () => this.setState({ done: true }),
      },
      h(TextField, { label: "Name", longest: 20, value: this.state.name, onChange: (name: string) => this.setState({ name }) }),
      h(RadioGroup, { label: "Size", items: ["Small", "Medium", "Large"], value: this.state.size, onChange: (size: number) => this.setState({ size }) }),
      h(Paragraph, { text: `${this.state.name || "(no name)"}, ${["small", "medium", "large"][this.state.size] ?? ""}. Green finishes.`, foreground: Colour.White }),
    );
  }
}

export function inputPages(): PageSpec[] {
  const next = (...links: (readonly [string, number])[]): ReturnType<typeof prompts> => prompts(["Index", INDEX], ...links);
  return [
    {
      page: BOXES,
      title: "Check boxes and radios",
      root: () =>
        frame(
          LOOK.input,
          "Check boxes, radios",
          next(["Switches", SWITCHES], ["Numbers", NUMBERS], ["Fields", FIELDS]),
          undefined,
          h(
            Heard,
            {},
            h(Checkbox, { label: "Email alerts" }),
            h(Checkbox, { label: "Auto-save", initial: true, sounds: SOUNDED }),
            h(Checkbox, { label: "Some permissions (mixed)", initial: "mixed" }),
            h(Checkbox, { label: "Read-only setting", disabled: true, initial: true }),
            h(Rule, { title: "a group in two columns", foreground: Colour.Blue }),
            h(CheckboxGroup, { items: ["Dark theme", "Compact", "Large type", "Captions"], columns: 2, initial: [0], disabledItems: [3] }),
            h(Rule, { title: "radio groups", foreground: Colour.Blue }),
            h(RadioGroup, { items: ["Daily", "Weekly", "Monthly"], initial: 1, sounds: SOUNDED }),
            h(RadioGroup, { label: "Theme", items: ["Light", "Dark", "System"], across: true }),
            h(RadioGroup, { label: "Locked", items: ["A", "B"], across: true, disabled: true }),
          ),
        ),
    },
    {
      page: SWITCHES,
      title: "Switches and sliders",
      root: () =>
        frame(
          LOOK.input,
          "Switches, sliders",
          next(["Boxes", BOXES], ["Numbers", NUMBERS], ["Fields", FIELDS]),
          undefined,
          h(
            Heard,
            {},
            h(Switch, { label: "Network access" }),
            h(Switch, { label: "Notifications", initial: true, sounds: SOUNDED }),
            h(Switch, { label: "Telemetry", disabled: true }),
            h(Switch, { label: "Short", labelCells: 10, initial: true }),
            h(Rule, { title: "sliders", foreground: Colour.Blue }),
            h(Slider, { label: "Volume", min: 0, max: 100, initial: 50, unit: "%" }),
            h(Slider, { label: "Brightness", min: 0, max: 100, step: 5, initial: 65, filled: true, foreground: Colour.Yellow, sounds: SOUNDED }),
            h(Slider, { label: "Balance", min: -10, max: 10, initial: 0, foreground: Colour.Green }),
            h(Slider, { label: "Locked", min: 0, max: 10, initial: 3, disabled: true }),
          ),
        ),
    },
    {
      page: NUMBERS,
      title: "Numbers and selects",
      root: () =>
        frame(
          LOOK.input,
          "Numbers, selects",
          next(["Switches", SWITCHES], ["Fields", FIELDS], ["Suggest", SUGGEST]),
          undefined,
          h(
            Heard,
            {},
            h(NumberInput, { label: "Quantity", min: 0, max: 99, initial: 3, sounds: SOUNDED }),
            h(NumberInput, { label: "Port", min: 1, max: 65_535, initial: 8098, step: 1 }),
            h(NumberInput, { label: "By tens", min: 0, max: 200, step: 10, initial: 50 }),
            h(NumberInput, { label: "Locked", min: 0, max: 9, disabled: true }),
            h(Rule, { title: "selects", foreground: Colour.Blue }),
            h(Select, { label: "Region", items: ["Europe", "Asia", "Americas", "Africa", "Oceania"], sounds: SOUNDED }),
            h(Select, { label: "Country", items: COUNTRIES, initial: 4, shown: 5 }),
            h(Select, { label: "Locked", items: ["Only this"], disabled: true }),
          ),
        ),
    },
    {
      page: FIELDS,
      title: "Text fields",
      root: () =>
        frame(
          LOOK.input,
          "Text fields",
          next(["Numbers", NUMBERS], ["Suggest", SUGGEST], ["Form", FORM]),
          undefined,
          h(
            Heard,
            {},
            h(TextField, { label: "Name", longest: 24, placeholder: "type your name" }),
            h(TextField, { label: "Password", longest: 16, masked: true, placeholder: "echoed as *" }),
            h(TextField, { label: "Email", longest: 24, validate: (text: string) => (/@.+\./u.test(text) ? null : "not an address") }),
            h(TextField, { label: "Locked", longest: 10, disabled: true }),
            h(TextField, { longest: 30, background: Colour.Magenta, placeholder: "no label, magenta", keep: true, onSubmit: () => {} }),
            h(TextField, { label: "Green", longest: 20, background: Colour.Green, initial: "given to begin with" }),
          ),
        ),
    },
    {
      page: SUGGEST,
      title: "Suggestions",
      root: () =>
        frame(
          LOOK.input,
          "Autocomplete, combo",
          next(["Fields", FIELDS], ["Form", FORM], ["Dialogs", DIALOGS]),
          undefined,
          h(Heard, {}, h(Autocomplete, { label: "Country", items: COUNTRIES, shown: 5, placeholder: "type a letter" }), h(Gap, {}), h(Autocomplete, { label: "Anywhere", items: COUNTRIES, shown: 4, anywhere: true, showAll: true, empty: "no country has that" })),
        ),
    },
    { page: FORM, title: "A form", root: () => frame(LOOK.input, "A form", next(["Suggest", SUGGEST], ["Dialogs", DIALOGS], ["Wizard", WIZARD]), undefined, h(FormDemo, {})) },
    { page: DIALOGS, title: "Dialogs", root: () => frame(LOOK.input, "Dialogs", next(["Form", FORM], ["Wizard", WIZARD], ["Boxes", BOXES]), undefined, h(DialogDemo, {})) },
    { page: WIZARD, title: "A wizard", root: () => frame(LOOK.input, "A wizard", next(["Dialogs", DIALOGS], ["Form", FORM], ["Boxes", BOXES]), undefined, h(WizardDemo, {})) },
  ];
}
