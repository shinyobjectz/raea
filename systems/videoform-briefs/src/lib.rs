//! What was asked and how it was made (docs/videoform.feature VF8f). A videoform's references come in groups (a brand
//! and its round), each with what was said of it: the person's own words as they gave the videos, and the documents
//! given with them (a brief, a brand's guides, notes). The context says what matters; the references show how it's
//! done. So each document is read into passages (passages.rs), each passage is found where its group's videos carry
//! it out and how (link.rs, by EmbeddingGemma 2: what's said and shown in each shot), the passages that say the same
//! across groups are joined into asks whose making is the videoform's pattern, and what only a group asked is its
//! brand's. What a similarity can't tell (a never, an order, a choice) the person's AI writes as briefs.ts rules, each
//! measured on its group's videos (rules.rs). The person's words outrank a document's, and a newer document an older.

pub mod link;
pub mod passages;
pub mod rules;
