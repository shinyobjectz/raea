// The launch story of docs/story.md, used by every test.
import { choose, story, value } from "../src/model.ts";

export const launch = story({
  title: "raea cloud",
  idea: { value: "freedom", side: "your recipe, run for you, frees you", counter: "only hands-on work is good work" },
  values: { freedom: value("buried", "free"), trust: value("doubt", "trust") },
  facts: { backlog: "the backlog is out of hand", recipeRuns: "the recipe can run without them" },
  agents: {
    team: { want: "ship every client's ads this week", need: "stop being the bottleneck", lie: "if I don't touch it, it's worse", role: "customer", mask: "hero" },
    raea: { mask: "mentor", role: "brand" },
  },
  world: {
    week: { who: "team", does: "edits the same ad again", turns: { freedom: -1 }, routine: true },
    doubles: { who: "team", does: choose(["the backlog doubles", "a client wants 50 versions"]), because: "week", sets: ["backlog"], turns: { freedom: -2 } },
    tries: { who: "team", does: "hands raea the recipe once", because: "doubles", needs: ["backlog"], turns: { trust: -1 }, dilemma: ["keep cutting by hand", "trust the recipe to run"] },
    ships: { who: "team", with: ["raea"], does: "ships fifty checked cuts by Friday", because: "tries", sets: ["recipeRuns"], turns: { trust: 2, freedom: 3 }, ends: { team: "need" }, claim: "fifty checked cuts", proof: ["recipeRuns"] },
  },
  tellings: {
    "30s": [
      { shows: ["ships"], feel: "curiosity", hides: ["recipeRuns"], asks: ["recipeRuns"], setup: "how" },
      { shows: ["week", "doubles"], feel: "recognition" },
      { shows: ["tries"], feel: "concern" },
      { shows: ["ships"], feel: "relief", reveals: ["recipeRuns"], pays: "how" },
    ],
  },
  lenses: ["mckee", "storynomics", "pas"],
});
