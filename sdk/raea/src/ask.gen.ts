// Generated from raea-core-ask (core/ask) by its `declarations`; edit the Rust, not this.

export type Question = { 
/**
 * letters, digits, `_`, `.` and `-`
 */
id: string, 
/**
 * the question in words, as clef reads it
 */
ask: string, type?: Kind, 
/**
 * choose: its options; grade and rate: its levels, lowest first (a grade's are LEVELS unless given)
 */
options?: Array<string>, 
/**
 * what an option (or yes and no) means, when its name alone doesn't say
 */
means?: { [key in string]: string }, 
/**
 * the answers that mean a problem: yes's "yes" (the default) or "no"; choose's options (none: a pick, no problem)
 */
flags?: Array<string>, sees?: Sees, 
/**
 * whether it listens to the sound (true unless set)
 */
hears?: boolean, 
/**
 * what the state tells clef beside the media: brief, words (heard), screen-words, measures, brand, platform; and
 * with: it compares the file with the files set beside it, so it isn't asked without them
 */
uses?: Array<string>, when?: When, 
/**
 * the question asked in its place when `when` doesn't hold (sound -> sound-silent)
 */
swap?: string, window?: Window, picks?: Picks, 
/**
 * words in a person's note that mean they saw this problem (AK6)
 */
label_words?: Array<string>, calibration?: Calibration, 
/**
 * what to change when it flags
 */
fix?: string, };

export type Kind = "yes" | "choose" | "grade" | "rate";

export type Sees = "video" | "stills" | "nothing";

export type Picks = "all" | "after-first";

export type When = { 
/**
 * the brief's modes it's for (pitch, story, explain, play, tool); empty is every mode
 */
modes?: Array<string>, 
/**
 * true: only when someone is heard speaking; false: only when no one is
 */
voice?: boolean, 
/**
 * true: only for a game; false: never for one
 */
game?: boolean, };

export type Window = { from?: number, to?: number, };

export type Calibration = { auc: number, n: number, positives: number, 
/**
 * the problem score at which it flags (10% false positives on its labels)
 */
threshold?: number, 
/**
 * YYYY-MM-DD
 */
date: string, 
/**
 * whose labels: "raea" (raea's bench, shipped) or "you"
 */
by: string, };

export const SETS = ["rubric", "av", "defects", "platform", "decide", "make"] as const;
