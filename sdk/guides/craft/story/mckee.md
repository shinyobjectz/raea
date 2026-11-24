# McKee: the shape of a designed story

Robert McKee, *Story: Substance, Structure, Style and the Principles of Screenwriting* (1997). In our words:

- **A story is a series of turns of value.** Every scene moves something the characters care about from one pole to
  the other (alive and dead, free and trapped, trusted and doubted). A scene that turns nothing is description.
- **The inciting incident** throws the protagonist's life out of balance and gives them something to want back.
- **Progressive complications** make getting it cost more each time; they escalate, they don't repeat.
- **The crisis** is a true dilemma: two goods, or the lesser of two evils. It's where character shows.
- **The climax** answers it, with the protagonist acting, and turns the value the story is about.
- **The resolution** shows the world after.
- **The controlling idea** is what the story argues, and the counter-idea is what it argues against.

## The lens (`mckee`)

Stages: `inciting`, `complications`, `crisis`, `climax`, `resolution`. It infers them from your world: the inciting
incident is the first event that turns a value by 2 or more, the crisis the first dilemma after it, the climax the
biggest turn of the focus value after that. A beat's own `stage: { mckee: "crisis" }` wins.

It finds: no inciting incident (error), one past the first third (warn), no crisis (error; it proposes "X must choose:
want, or need"), no climax, a climax that doesn't turn the focus value or where the protagonist doesn't act (warn),
complications that ease off (warn).

Reach for it on anything with a protagonist and a decision. It's the default.
