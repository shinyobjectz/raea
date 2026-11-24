// The lenses raea ships, by name (docs/story.feature ST3a, ST3b).
import type { Lens } from "../lens.ts";
import { bernays } from "./bernays.ts";
import { campbell } from "./campbell.ts";
import { harmon } from "./harmon.ts";
import { kishotenketsu } from "./kishotenketsu.ts";
import { launch, LAUNCH_PRIORS } from "./launch.ts";
import { mckee } from "./mckee.ts";
import { pas } from "./pas.ts";
import { pixar } from "./pixar.ts";
import { propp } from "./propp.ts";
import { saveTheCat } from "./save-the-cat.ts";
import { storynomics } from "./storynomics.ts";
import { vogler } from "./vogler.ts";

export { bernays, campbell, harmon, kishotenketsu, launch, LAUNCH_PRIORS, mckee, pas, pixar, propp, saveTheCat, storynomics, vogler };
export const LENSES: Record<string, Lens> = { mckee, pixar, storynomics, pas, campbell, vogler, harmon, "save-the-cat": saveTheCat, kishotenketsu, propp, bernays, launch };
