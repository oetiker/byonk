// Prepends a "// byonk-editor-src-sha256: <hash>" stamp line to the built
// bundle and writes it to static/dev/editor.bundle.js, the file
// src/api/dev.rs embeds via include_str!. `make editor-check` recomputes
// the hash from source and compares it to this stamp to catch a bundle
// that was committed without being rebuilt.
import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const hash = execSync("sh hash.sh").toString().trim();
const body = readFileSync("dist/editor.bundle.js", "utf8");
writeFileSync("../editor.bundle.js", `// byonk-editor-src-sha256: ${hash}\n${body}`);
