// Generated assets as values (spec: "Generated assets as values"). lyria() and sao() only describe what to make;
// the renderer generates each one once, under the run's paid-generation cap, and caches it by id.

export type AssetKind = "lyria" | "sao";

export interface GeneratedAsset {
  kind: AssetKind;
  prompt: string;
  seconds: number;
  /** a stable hash of kind, prompt and seconds */
  id: string;
  /** "asset:<kind>/<id>" */
  src: string;
}

// Two independent 32-bit FNV-1a passes (different offset bases) give a 64-bit id as 16 hex digits.
function fnv1a(text: string, basis: number): string {
  let h = basis >>> 0;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function describe(kind: AssetKind, prompt: string, seconds: number): GeneratedAsset {
  if (typeof prompt !== "string" || prompt.trim() === "") throw new Error(`${kind}: give a prompt describing the sound`);
  if (typeof seconds !== "number" || !(seconds > 0) || !Number.isFinite(seconds)) {
    throw new RangeError(`${kind}("${prompt}"): seconds must be a positive number, got ${seconds}`);
  }
  const key = `${kind}\u0000${prompt}\u0000${seconds}`;
  const id = fnv1a(key, 0x811c9dc5) + fnv1a(key, 0x050c5d1f);
  return { kind, prompt, seconds, id, src: `asset:${kind}/${id}` };
}

/** Music to generate (30 seconds by default). */
export function lyria(prompt: string, options: { seconds?: number } = {}): GeneratedAsset {
  return describe("lyria", prompt, options.seconds ?? 30);
}

/** A sound effect to generate (2 seconds by default). */
export function sao(prompt: string, options: { seconds?: number } = {}): GeneratedAsset {
  return describe("sao", prompt, options.seconds ?? 2);
}

export function isGeneratedAsset(v: unknown): v is GeneratedAsset {
  return (
    typeof v === "object" &&
    v !== null &&
    ((v as GeneratedAsset).kind === "lyria" || (v as GeneratedAsset).kind === "sao") &&
    typeof (v as GeneratedAsset).src === "string"
  );
}
