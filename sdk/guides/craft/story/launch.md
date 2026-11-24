# Launch: the shape of a product's launch film

A launch film is the post that says a product exists: the trouble in the viewer's own words, the product's name, the
product doing its job, a named proof (a number, a benchmark, a customer, a raise), and how to start. That's the form
most launch films share, from consumer hardware to developer tools.

In our words, and from our own data: raea measured 156 pairs of launch films on X, each pair the best and the worst
launch of one account, so the audience is the same (11-22, the whatships catalog of 3,328 launch videos).

- The one thing that separated the better film was **length**: it was the longer film in 104 of 155 pairs (67%,
  p < 0.001); the medians were 50.5 s against 33.1 s. Films of 40 to 60 s were the better of their pair 55% of the
  time; 20 to 40 s, 35%; over 90 s, 73%.
- A voice speaking leaned the same way (40% of the better films, 29% of the worse) but didn't survive correcting for
  the 23 measures tested. Nothing about the cut (cuts a second, motion, sync to music, colour, the first cut) separated
  them at all.
- A blind read of 30 pairs picked the better film 23 times, but always picking the longer one gets 25. What the reader
  went on was the size of the news: a flagship release over a variant, a raise, a famous face.

So length is a marker of big news, not a lever. Don't pad a film to reach 50 seconds, and don't cut one that needs a
minute. Make the news big and the film as long as it needs.

## The lens (`launch`)

Stages: `trouble` (a beat that turns something down for the viewer, before the product), `name` (the first beat with
the brand's agent), `works` (the product turning things up), `proof` (an event that makes a claim's proof true),
`start` (the last beat).

It finds: no trouble (warn), trouble later than the first third (note), no agent with the brand role (note), a product
that never does its job on screen (warn), no named proof (note), and the measured priors as notes: a telling shorter
than three in four of the better launches (when every beat has `seconds`), and nobody speaking. `LAUNCH_PRIORS` holds
the numbers.

Reach for it with `storynomics` (the customer is the hero) or `pas` on a short one. Hold a top real launch's shape
against yours with `raea fingerprint` too, never its beats.

Source: raea's own measure of the whatships.com launch catalog, docs/story.md ("What it found").
