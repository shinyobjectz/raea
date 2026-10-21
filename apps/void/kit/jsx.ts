// The JSX runtime space.tsx is read with: elements as plain objects, nothing rendered.
export const Fragment = Symbol.for("raea.void.fragment");
export function jsx(type: unknown, props: Record<string, unknown>, key?: unknown) {
  return { type, props: props ?? {}, key };
}
export const jsxs = jsx;
export const jsxDEV = jsx;
