# Session browser and prompt outline — local verification

- Session picker: titles on the left, saved content on the right, independent
  scrolling, metadata header, and explicit Enter/New/Close actions. Selecting
  a title only previews; it does not stop the current task or acquire Runtime
  ownership. Delayed responses cannot replace a newer selection.
- Native `sessions read` and session-detail responses now carry additive,
  derived `display_user_prompt` fields. Both reuse the existing native
  `classify_user_turn_prompt` and structural metadata recognition. Runtime
  events/tool results are not prompts; user-authored tag examples survive.
- GUI browse, select, and post-turn refresh share the saved-message display
  projection. Desktop session transport includes this canonical snapshot so
  older seeded Runtime item text cannot reintroduce background envelopes.
  Persisted/model-facing messages and the KV-cache prefix are unchanged.
- This requires the newly built GUI and Agent together. Old Agent payloads
  fail explicitly instead of silently treating background text as user input.

Verification performed:

- Offline native/UI compilation and release WASM build succeeded.
- Desktop Rust tests: `test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s`.
- `python desktop/tests/user_prompt_projection.py target/debug/codewhale.exe`:
  five expected prompts retained, runtime metadata/background events/tool
  results excluded, literal user examples preserved, source history bytes
  unchanged. Real stdio resume/save/read retained the same prompt list.
- The same regression script failed against the prior preview.7.7 packaged
  Agent at the missing native provenance assertion.
- `python desktop/tests/mock_conversation.py target/debug/codewhale.exe --native-sessions --reasoning`:
  actual Runtime reasoning/answer stream, tool history restoration,
  cross-workspace rejection, and temporary fixture execution passed.
- Local browser with compiled WASM and mock IPC: left/right layout in light
  and dark themes, 640px width, long titles/content, independent scroll areas,
  rapid selection with a delayed response, empty history, failed reads,
  disabled entry on failure, close/reopen, and explicit session entry.

Browser tests do not constitute native installed-WebView acceptance. No new
installer was built in this iteration; preview.7.7 remains the earlier package.
