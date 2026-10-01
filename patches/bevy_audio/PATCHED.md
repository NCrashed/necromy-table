bevy_audio 0.19.1 from crates.io, patched for necromy-table:
`src/audio_output.rs` reopens the output when the default device changes
(on Linux also when the sound cards change), when the stream fails, or on
`ReopenAudioOutput`; every playing sink starts again on the new output.
`src/lib.rs` registers the system and the message. Rebase on a new Bevy by
copying the new crate and carrying these two changes over.
