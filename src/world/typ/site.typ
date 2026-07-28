// `@baudelaire/site`: site identity as typed bindings.
//
// Generated and served by baudelaire; nothing for this exists on disk. Import
// it as `#import "@baudelaire/site:0.1.0": title, url`. A name that is not
// exported fails at the import instead of reading back `none`.
//
// Config-derived values only. Build metadata that changes between builds lives
// at `sys.inputs.baudelaire` (`.git`, `.date`), where baudelaire tracks reads
// per page: a value baked in here would rebuild the whole site on every commit.
// Every key below is always bound, so an unset config value is `none` rather
// than a missing name.
