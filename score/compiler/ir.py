"""The intermediate form between a score script and any target: what the piece is, resolved against the takes and the
footage, with nothing in it a target decides.

  Script     what the writer wrote, parsed: sections, lines (quoted words, delivery, punch), the parts and style notes on
             each line, the stylesheet. Made by front.from_graph from a .brief's intent graph
             (raea-brief parse).
  Timeline   the script placed in time on real footage: each kept line as a clip (the take, its source span at a clean
             edit point, its edit frames, its framing), each part with its frames and its placement, captions, sound
             cues. Made by front.resolve from the Script, the space's transcripts and its media.

Placement is written the way the targets read it: `place` is an expression over CSS variables (var(--band-top),
max(var(--band-low), var(--face-bottom) + 40px)), with `y` its value on this footage, so a part placed by
the compiler gets y and a component that places itself (<Region>) gets the band (an agent session, 2026-10-16: perception-relative placement as CSS
variables; no anchor positioning). Framing is the same: `vars` per clip carries --face-top/--face-bottom in edit pixels
beside the pan that put them there.
"""
from dataclasses import asdict, dataclass, field


@dataclass
class Note:
    """one [[ ... ]]: a part (kind and props) with its utilities, or utilities alone"""
    kind: str | None = None          # lockup, captions, statcard, notthat, follow, component, or None (style only)
    props: dict = field(default_factory=dict)
    utils: list = field(default_factory=list)
    anchor: str | None = None         # on:'words'
    unknown: list = field(default_factory=list)


@dataclass
class Line:
    role: str | None                  # the section's role (hook, proof...)
    take: str | None                  # the scene heading's take, a hint
    text: str                         # the quoted words
    delivery: str | None = None       # the parenthetical
    punch: object = False             # PUNCH IN 1.2X: before it (the value), "unset" when no value was given
    notes: list = field(default_factory=list)
    alts: list | None = None          # `first read || second read`: the alternatives sheet.pick_alternatives chose from
    claims: list = field(default_factory=list)  # claim:ID on the line's notes


@dataclass
class Script:
    title: str = ""
    format: str = ""                  # the Format line as written
    secs: tuple | None = None         # (low, high) the Format allows
    sections: list = field(default_factory=list)
    lines: list = field(default_factory=list)
    screen_text: list = field(default_factory=list)
    style: dict = field(default_factory=dict)   # selector -> {property: value}, from the stylesheet
    text: str = ""                    # the source, for the sheet
    shape: tuple | None = None        # (width, height) from the deliverable's ratio, when the script came from a .brief
    beats: list = field(default_factory=list)   # a .brief's open beats (~ lines), not yet bound to footage
    checks: list = field(default_factory=list)  # a .brief's ! rules
    claims: dict = field(default_factory=dict)  # $claim.ID: {text, source}


@dataclass
class Clip:
    name: str
    take: str
    file: str                         # media path relative to the space
    src_in: float                     # seconds into the take
    src_out: float
    start: int                        # edit frames
    end: int
    line: int                         # index into Script.lines
    zoom: float | None = None
    pan_x: float | None = None
    pan_y: float | None = None
    vars: dict = field(default_factory=dict)    # --face-top, --face-bottom (edit px)


@dataclass
class Part:
    kind: str                         # a kit part, or "component" (void's component tool)
    name: str
    props: dict
    start: int
    end: int
    band: str | None = None           # top / low / auto
    place: str | None = None          # the placement as a CSS expression
    y: int | None = None              # its value on this footage
    component: str | None = None      # for kind "component": the component's name (components/<name>.tsx)
    sfx: str | None = None            # a foley prompt for its first frame (sfx:'...')
    motion: list = field(default_factory=list)  # the note's utilities: enter-*, exit-fade, settle, dur-*, ease-*


@dataclass
class Sound:
    kind: str                         # bed / fx
    prompt: str
    model: str                        # lyria / sao (the generation policy)
    seed: int
    start: int
    secs: float
    gain: float


@dataclass
class Timeline:
    fps: int
    width: int
    height: int
    frames: int
    clips: list = field(default_factory=list)
    parts: list = field(default_factory=list)
    sounds: list = field(default_factory=list)
    words: list = field(default_factory=list)   # the kept words in edit time (start_ms, end_ms, text), for cue timing
    takes: dict = field(default_factory=dict)   # take -> media path
    vars: dict = field(default_factory=dict)    # edit-wide: --band-top, --band-low, --caption-y
    dropped: list = field(default_factory=list) # lines no take with picture says
    notes: list = field(default_factory=list)   # what the footage changed, and what the script asked for that couldn't be done
    refused: list = field(default_factory=list) # parts the space doesn't have
    background: str | None = None     # the script's or the brand's; none means the renderer's own
    kit: str = "space"                # whose kit the parts came from (the space's, or the global one)


def to_json(x):
    return asdict(x)
