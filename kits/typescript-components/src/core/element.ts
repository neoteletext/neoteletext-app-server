// Elements: what a component's render answers, as React's are. An element
// names a component class and the props to make or update it with; the
// mount (mount.ts) turns a tree of them into component instances, keeping
// an instance wherever the same class is rendered in the same place.

import type { Component } from "./component.ts";

/** The props every element may carry, read by the parent's layout (layout.ts). */
export interface LayoutProps {
  /** A fixed width, in cells. */
  readonly width?: number;
  /** A fixed height, in rows. */
  readonly height?: number;
  /** A share of what is left over in a stack or a row of columns. */
  readonly grow?: number;
}

/** What a component's render may answer: elements, text (a line in white), nothing, or lists of these. */
export type Child = Element | string | number | boolean | null | undefined | readonly Child[];

/** Props with the children an element was given. */
export interface WithChildren {
  readonly children?: readonly Child[];
}

/** A component class, made with its props. */
export type ComponentClass<P extends object> = new (props: P) => Component<P, object>;

/** One element: a component class, its props, children included, and its key among its siblings. */
export interface Element {
  readonly type: ComponentClass<never>;
  readonly props: WithChildren & LayoutProps;
  readonly key: string | number | undefined;
}

/** What `h` takes as props: the component's own, the layout's, and a key. */
export type PropsOf<P extends object> = Omit<P, "children"> & LayoutProps & { readonly key?: string | number };

/**
 * The props a component class is made with, read from the class itself
 * rather than inferred from the props given, so a class generic over its
 * items (a list of anything) is used without naming its type, its type
 * parameter then its constraint, as `ChoiceList<Round>` names one.
 */
export type ClassProps<C> = C extends new (props: infer P) => unknown ? (P extends object ? P : never) : never;

/** Any component class, for `h` to read its props from: a class made with props whose instances keep them. */
export type AnyComponentClass = new (props: never) => { readonly props: object };

/**
 * An element of `type` with `props` and `children`, as React's
 * createElement: the class-component form, with no JSX, since Node runs
 * the channel's TypeScript as it is.
 */
export function h<C extends AnyComponentClass>(type: C, props: PropsOf<ClassProps<C>>, ...children: Child[]): Element {
  const { key, ...rest } = props as PropsOf<ClassProps<C>> & { readonly key?: string | number };
  const withChildren = children.length > 0 ? { ...rest, children } : rest;
  return { type: type as unknown as ComponentClass<never>, props: withChildren as WithChildren & LayoutProps, key };
}

/** Whether `child` is an element rather than text or nothing. */
export function isElement(child: Child): child is Element {
  return typeof child === "object" && child !== null && !Array.isArray(child) && "type" in child;
}

/** `children` as a flat list: lists opened, nothing and booleans dropped, text and numbers kept. */
export function flatten(children: Child): (Element | string)[] {
  const out: (Element | string)[] = [];
  const visit = (child: Child): void => {
    if (child === null || child === undefined || typeof child === "boolean") {
      return;
    }
    if (Array.isArray(child)) {
      (child as readonly Child[]).forEach(visit);
      return;
    }
    if (typeof child === "string" || typeof child === "number") {
      out.push(String(child));
      return;
    }
    out.push(child as Element);
  };
  visit(children);
  return out;
}
