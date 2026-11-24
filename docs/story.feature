@spec
Feature: Story: a story as ECS data, read through many frameworks, generated without a fixed answer, checked from tsc to the outcome
  Shane, 2026-11-22: "a story skill tree that takes the hero's journey of a thousand faces, pixar story principles eg.
  creativity inc, robert mckee's storynomics, propaganda by edward bernays, etc. ... an open non-deterministic story
  framework for videos/games/dialogue ... a new type of story typescript sdk that also integrates with the
  screenplay/avscript stuff but at a higher level so you could test and validate story beats in the code or across
  themes". Then: "it should live in brief and be an import ... keep it open and explicitly used. story sdk can stay
  typescript", and "build small language models or generative story models that are able to chart out things a brief
  might want or need or to check if a story is right ... take the ecs, story as data approach by storm".
  The plan, the state of the art and the reasons: docs/story.md.

  The model this file sets:
    1. A story is data in three layers: the world (facts and the events that change them, with causes), tellings
       (beats: which events the audience sees, in what order, what each reveals), and the render (script.ts).
    2. Its parts are entities with components in a small ECS; systems derive what is never written (value curves,
       knowledge, suspense and mystery, the cause graph, beliefs, tension). Checks read only what's derived.
    3. Frameworks are lenses: stages, a map onto beats, checks and proposals. A story is read through all of them.
    4. Nothing reads a story unless something imports it: the brief's `story:`, a composition, a check, a drama manager.
    5. The SDK is TypeScript with no runtime dependencies. Rust only lets a story through and points back at it.

  Background:
    Given the package sdk/story (@raea/story, "raea/story" in sdk/tsconfig.base.json, which raea's bundler follows)
    And its tests: sdk/story/test/*.test.ts (bun test)
    And the launch corpus ~/.vd/launch (whatships' launch videos, X engagement, same-account pairs): the first data

  Rule: ST0. The model: world, tellings and render as typed data

    Scenario: ST0a. A story module type-checks its own references
      Given a story written with story() from "raea/story"
      When an event names a cause, a fact, an agent or a value the story doesn't declare
      Or a beat shows an event, sets up or pays off a key the story doesn't declare
      Then tsc refuses it, naming the key
      And the plan:
        | crate | file                 | lines | job                                                          |
        | sdk   | sdk/story/src/model.ts         | 300   | the types and story(): values, facts, agents, events, tellings |
        | sdk   | sdk/story/src/index.ts         | 40    | the exports                                                    |
        | sdk   | sdk/story/test/types.ts        | 120   | references refused by tsc (expect-error) and accepted ones     |
        | sdk   | sdk/story/test/fixture.ts      | 120   | the launch story every test uses                               |

    Scenario: ST0b. A story is an ECS world
      Given a story value
      When it's loaded into a world
      Then every value, fact, agent, event and beat is an entity with its components, and a query by components finds
        them, in the telling's order for beats
      And the plan:
        | crate | file             | lines | job                                                  |
        | sdk   | sdk/story/src/ecs.ts       | 160   | entities, components, resources, queries, systems     |
        | sdk   | sdk/story/src/load.ts      | 160   | a story and a telling into a world                    |
        | sdk   | sdk/story/src/resolve.ts   | 120   | choice points resolved with a seed (mulberry32)       |

    Scenario: ST0c. The brief imports stories, and a script's scene names its beat
      Given brief({ ..., story: launch }) or story: { launch, quest1 }
      Then raea reads the brief with its story kept as it is (Brief.story, opaque to Rust)
      And a script's scene may carry beat: "<id>" (compileScript writes it)
      And the plan:
        | crate           | file                       | lines | job                         |
        | raea-core-brief | core/brief/src/brief.rs    | 140   | Brief.story, kept as written |
        | raea-core-brief | core/brief/src/script.rs   | 90    | Scene.beat                   |

  Rule: ST1. Systems derive what's never written

    Scenario: ST1a. Value curves, the cause graph, knowledge, beliefs and tension
      Given a world loaded from a story and one of its tellings
      When the systems run
      Then each value has its curve over the telling, and the curve's shape is named (rise, fall, fall-rise, rise-fall,
        rise-fall-rise, fall-rise-fall)
      And the audience's knowledge and each agent's are known at every beat, with suspense (the audience knows a fact an
        agent on screen doesn't), mystery (an open question the audience is waiting on) and surprise (a reveal nothing
        set up)
      And an event whose agent acts on a fact they don't hold yet is a belief break
      And the tension at each beat comes from open questions and value pressure
      And the plan:
        | crate | file                  | lines | job                                                   |
        | sdk   | sdk/story/src/systems/values.ts | 140   | value curves and their shapes                          |
        | sdk   | sdk/story/src/systems/know.ts   | 200   | audience and agents' knowledge, suspense, mystery, surprise, belief breaks |
        | sdk   | sdk/story/src/systems/cause.ts  | 120   | the cause graph: uncaused beats, dead ends             |
        | sdk   | sdk/story/src/systems/tension.ts| 100   | the tension curve                                      |

  Rule: ST2. Exact checks

    Scenario: ST2a. A story is checked with no model
      When checkStory runs
      Then it finds: a beat that turns no value, an event with no cause (after the first), an unpaid setup, a payoff with
        no setup, a belief break, an unmeant surprise, a run of beats all turning one way, an arc left open
      And each finding names the beat or event, says what's wrong in a sentence, and suggests a fix
      And the plan:
        | crate | file             | lines | job                          |
        | sdk   | sdk/story/src/checks.ts    | 260   | the exact checks and findings |
        | sdk   | sdk/story/test/checks.test.ts | 200 | the launch story passes; derived curves and questions; seeds |

  Rule: ST3. Lenses

    Scenario: ST3a. A lens maps, checks and proposes
      Given the lenses mckee, pixar, storynomics and pas
      When a story is read through each
      Then each says which beats play its stages, what's missing or out of order, and proposes beats for what's missing
      And a story read through every lens says which it passes
      And the plan:
        | crate | file                                | lines | job                                                        |
        | sdk   | sdk/story/src/lens.ts               | 200   | the lens shape, the shared structure (hero, brand, inciting, crisis, climax), readThrough |
        | sdk   | sdk/story/src/lenses/index.ts       | 40    | the shipped lenses by name                                 |
        | sdk   | sdk/story/src/lenses/mckee.ts       | 120   | inciting, complications, crisis, climax, resolution        |
        | sdk   | sdk/story/src/lenses/pixar.ts       | 120   | the story spine                                            |
        | sdk   | sdk/story/src/lenses/storynomics.ts | 120   | the customer as hero, the brand as mentor or gift          |
        | sdk   | sdk/story/src/lenses/pas.ts         | 120   | problem, agitate, solve                                    |
        | sdk   | sdk/story/test/lenses.test.ts       | 150   | the launch story passes; a brand-hero fails; a crisis proposed |

    Scenario: ST3b. The rest of the frameworks
      Given campbell, vogler, harmon, save-the-cat, kishotenketsu, propp and bernays as lenses
      Then each has its stages, checks and proposals, and bernays only arranges true proof (a claim with no fact behind
        it is a finding)
      And the plan:
        | crate | file                                   | lines | job                                                 |
        | sdk   | sdk/story/src/lenses/common.ts         | 40    | who acts where, masks, the lowest beat              |
        | sdk   | sdk/story/src/lenses/campbell.ts       | 60    | call, aid, threshold, ordeal, boon, return          |
        | sdk   | sdk/story/src/lenses/vogler.ts         | 60    | twelve stages; masks checked by what they do        |
        | sdk   | sdk/story/src/lenses/harmon.ts         | 50    | the circle: the price and the change                |
        | sdk   | sdk/story/src/lenses/save-the-cat.ts   | 50    | the beat sheet as shares of the telling             |
        | sdk   | sdk/story/src/lenses/kishotenketsu.ts  | 40    | the late turn, no conflict asked for                |
        | sdk   | sdk/story/src/lenses/propp.ts          | 50    | lack, test, gift, struggle, the lack made good      |
        | sdk   | sdk/story/src/lenses/bernays.ts        | 60    | proof seen, a witness, the customer's want; never invented |
        | sdk   | sdk/story/src/lens.ts                  | 110   | levels and between in the structure                  |

  Rule: ST4. Generation without a fixed answer

    Scenario: ST4a. Choice points, seeds, pruning
      Given a story with choose([...]) in place of fixed parts
      When it's sampled with a seed
      Then the same seed gives the same story, a new seed may give another, and no sampled story fails an exact check
      And a theme matrix runs one story pattern across many premises and says where it breaks
      And the plan:
        | crate | file                          | lines | job                                          |
        | sdk   | sdk/story/src/sample.ts       | 160   | sample, samples, matrix, judge               |
        | sdk   | sdk/story/test/sample.test.ts | 150   | sampled stories pass; the matrix; compilers  |

  Rule: ST5. Into raea check, scripts and games

    Scenario: ST5a. The SDK's checks run in raea check
      Given a project whose brief imports a story and whose composition names its Sequences by beat id
      When raea check runs
      Then the story-only, on-screen and render checks run as ordinary raea/check checks
      And a story is checked with no composition: raea check NAME.story.ts runs every check over "story" or "both" once,
        with no frames
      And the plan:
        | crate              | file                              | lines | job                                             |
        | sdk                | sdk/story/src/raea/checks.ts      | 120   | storyCheck: the story's findings and its beats on screen |
        | sdk                | sdk/story/src/raea/compose.tsx    | 80    | beatTimes and <Beat>                             |
        | raea-app-direction | apps/direction/src/check.rs       | 420   | raea check NAME.story.ts                         |
        | raea-app-direction | apps/direction/src/ts.rs          | 120   | a check's `over`                                 |

    Scenario: ST5b. A telling compiles to a script, to storylets and to dialogue
      Then compileScript gives raea/brief's Script, each scene carrying its beat
      And compileStorylets gives preconditions and effects over facts, with a drama manager that picks the next storylet
        to keep a target tension curve
      And dialogue lines carry what's said, what's meant, and the value they turn
      And the plan:
        | crate | file                    | lines | job                                                         |
        | sdk   | sdk/story/src/compile.ts | 200  | Script, storylets and a drama manager, Ink and Yarn          |

  Rule: ST6. Story models: small, on-device, schema-native

    Scenario: ST6a. Exact labels by mutation
      Given valid stories
      When mutate() breaks one (drops a setup, breaks a cause, gives an agent a fact early, flattens a value, reorders
        a reveal)
      Then the broken story and its label (what was broken, where) are a training row, and checkStory finds it
      And the plan:
        | crate | file                          | lines | job                                                |
        | sdk   | sdk/story/src/mutate.ts       | 300   | every way a story told one way can be broken, labelled |
        | sdk   | sdk/story/test/mutate.test.ts | 120   | every mutant with an expected check is found by it  |

    # Paused 11-22 (Shane: "screw the model, just finish and wrap the sdk"); models/ keeps the code and ~/.vd/story the data.
    Scenario: ST6b. The extractor, the generator and the critic
      Given small bases (about 2B parameters or less), trained with MLX on this Mac and exported to ONNX for ort
      Then the extractor turns a transcript into story data, the generator proposes what a brief wants as data that
        passes the schema, and the critic says whether a story is right and where
      And each is measured held out against a zero-shot baseline and a frontier LLM judge, with the numbers written here

  Rule: ST7. The launch corpus

    @done
    Scenario: ST7a. What the better launch of a pair does
      Given 158 same-account pairs of launch films, measured and transcribed
      Then the paired comparison and a blind read of 30 pairs say whether craft shows at all, with numbers
      And what holds up becomes a launch profile in raea fingerprint and the launch lens's priors

  Rule: ST8. The skill tree

    @done
    Scenario: ST8a. craft/story in the raea skill
      Then craft/story/ holds a router and a page per framework in our own words with sources, and how to write, check,
        sample and score a story with raea/story
      And raea guide story/router prints it, and the skill carries each page as a copy
      And the plan:
        | crate    | file                                  | lines | job                                        |
        | guides   | sdk/guides/craft/story/router.md      | 50    | which page, which lens                      |
        | guides   | sdk/guides/craft/story/write.md       | 90    | the three layers, into the project          |
        | guides   | sdk/guides/craft/story/check.md       | 60    | each finding and its fix                    |
        | guides   | sdk/guides/craft/story/score.md       | 50    | lenses, the critic, a real shape            |

  Rule: ST9. Any length

    @done
    Scenario: ST9a. Story lines
      Then a story reads and writes as one keyword-led line per value, fact, agent, event and beat (toLines, fromLines)
      And every valid corpus story reads back from its lines exactly, and a broken line names where
      And the plan:
        | crate | file                       | lines | job                                     |
        | sdk   | sdk/story/src/lines.ts     | 160   | to and from story lines                 |
        | sdk   | sdk/story/test/lines.test.ts | 80  | the round trip, broken lines, the corpus |

    Scenario: ST9b. Windows, not whole stories
      Given the state at a beat (what the audience knows, open questions, suspense, levels, tension, unpaid setups,
        unsettled wants, events ahead) stays about the same size at any length
      Then the generator writes a beat from its state and the beats around it, and the extractor reads a script a
        chunk at a time with what earlier chunks found, merged
      And on Birds the chained extractor beats the whole-story extractor, and the windowed generator beats its zero-shot
      And the plan:
        | crate | file                         | lines | job                                     |
        | sdk   | sdk/story/src/state.ts       | 120   | stateAt, windowAt                       |
        | sdk   | sdk/story/models/lib.ts      | 180   | chunkSteps, mergeSteps, stepSchema      |
        | sdk   | sdk/story/models/known.ts    | 30    | prompts for the chain, from raea/story  |

    Scenario: ST9c. A feature-length story
      Given a public-domain feature film told shot by shot, its scenes grouped
      Then the extractor chains through it and the checks and lenses run on what it reads
      And the generator writes scenes top-down from premise to world to scenes to shots, each level checked

  Rule: ST10. Proven against plain prompts

    @done
    Scenario: ST10a. Edge cases and fuzzing
      Given 29 odd and broken stories, each with what should happen, and valid stories with one field made the wrong type
      Then anything fromData accepts, every reader reads without crashing (fromData holds a story to storySchema)
      And unsets takes a fact back, two beats never share an id, seconds is never negative, a telling that shows
        nothing is a finding
      And the plan:
        | crate | file                       | lines | job                                        |
        | sdk   | sdk/story/bench/edges.ts   | 230   | the edge cases, run and reported           |
        | sdk   | sdk/story/bench/fuzz.ts    | 80    | wrong-typed fields through every reader    |
        | sdk   | sdk/story/src/data.ts      | 40    | shapeErrors: the schema as the reader's gate |

    @done
    Scenario: ST10b. The same prompts with and without it
      Given eight prompts written five ways by one writer (plain; plain and an editor; plain and raea/story as the
        editor; written as story data; story data then a screenwriter), judged blind by two models of other families
      Then raea/story as the editor of a draft ties a strong editing prompt and beats plain (26 of 32, p = 0.001)
      And written straight into story data a script reads worst (7 of 32 over plain), so the guides say: write the
        script, read it back as data, check it, fix by beat
      And the plan:
        | crate | file                         | lines | job                                     |
        | sdk   | sdk/story/bench/ab.ts        | 330   | the five arms, extraction, the judges   |
        | sdk   | sdk/story/bench/ab-report.ts | 140   | ranks, pairs, extraction, cost          |
