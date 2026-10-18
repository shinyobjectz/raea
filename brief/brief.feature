Feature: The brief, split into beats the source table can be searched with (the roadmap Track F)
  The founder, 2026-10-14: RAG over a table of raw source clips, "figuring out what should be generated roughly inside of
  them based on the feature brief and source footage and what we can gather". F1 is the first part: a brief, written
  however it was written (a Markdown brief, a list of script lines, notes from a call), split into beats, each one
  moment of the video, as rows F2 looks up against the footage.

  Rule: A beat is a row
    Scenario: What a beat holds
      Then a beat has a phrase (what happens), a kind (to-camera, b-roll, product, screen or end-card), what Perception
        must find for it (face, speech, product, hands, screen, text, place, action or logo: the facts and SigLIP columns
        F2 searches), the words to say when the brief gives them, the words of the brief it comes from, and which of the
        brief's units it uses
      And what no shot can show (a launch date, a deliverable count, a rate, an instruction) is set aside with a reason,
        so every unit of the brief is accounted for

  Rule: Splitting free text is the agent's job (raea-beats, 2026-10-15)
    Scenario: Ten briefs from the host app's eval scenarios, split by the agent's own model
      Given raea/brief (`raea-beats split BRIEF.md`, `raea-beats grade DIR`): the brief cut into units (lines, bullets,
        and clauses of free text), sent with the beat format to MiniMax-M3 through the host app's API (the Agent's route,
        with the eval user's token), the answer checked against the brief in Rust
      And the briefs: the ten distinct, non-adversarial ones in host/agent/evals (Glossier's Milky Jelly launch and its
        two-deliverable version, Client's founder takes in two forms and its script, Ferro's cold brew, the Ember
        blend's slow pours, notes from a call about oat latte cans, the glow serum, Oat & Arrow, the compare-scene
        brief); the red-team briefs left out (one tells the assistant to accept any rate, one carries bank and passport
        numbers)
      When each is split once
      Then 42 beats; 41 quote the brief word for word (the other joins two of its lines); all 42 use only the fixed
        kinds and finds; all 65 units are used by a beat or set aside (20 set aside: dates, deliverables, "keep it calm")
      And a script's lines become to-camera beats carrying their words ("We're not automating our creatives with AI.
        We're simulating them."), the call's "show the can cracking open" a product beat finding product, hands and
        action, and "they hate when creators say 'game changer'" is set aside as an instruction: a rule for the words,
        not a shot (kept for the script check, not for F2)
      And so F2 has its rows: a BRIEF.beats.json beside each brief; looking each beat up in the source table is next
