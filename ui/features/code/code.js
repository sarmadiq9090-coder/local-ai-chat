// code.js — code blocks in messages. The markup (language bar, "نسخ" and
// "تشغيل" buttons, highlight.js colors) is produced by core/markdown.js; this
// feature owns its styles (code.css) and installs the delegated copy handler.
// Its strings (copy, copied, run, runPreview…) live in core/strings.js because
// core/markdown.js renders them.

import { installCodeCopy } from "../../core/markdown.js";

export default { name: "code", init: () => installCodeCopy() };
