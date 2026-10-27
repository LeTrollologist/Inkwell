// Tiny sanity check for the no-bundler frontend.
// Usage: node scripts/check-ui.js
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const app = fs.readFileSync(path.join(root, "ui", "app.js"), "utf8");
const html = fs.readFileSync(path.join(root, "ui", "index.html"), "utf8");

const names = [...app.matchAll(/^\s*(?:async\s+)?function\s+(\w+)/gm)].map((m) => m[1]);
const dupes = [...new Set(names.filter((name, index) => names.indexOf(name) !== index))];

const referenced = [...new Set([...app.matchAll(/\$\("([^"]+)"\)/g)].map((m) => m[1]))];
const missing = referenced.filter((id) => !html.includes(`id="${id}"`));

const iconNames = Object.keys(
  Object.fromEntries([...app.matchAll(/([a-zA-Z]+):\s*'<svg/g)].map((m) => [m[1], true]))
).filter((name) => !app.includes(`ICONS.${name}`));

const voidTags = new Set([
  "area", "base", "br", "col", "embed", "hr", "img", "input",
  "link", "meta", "param", "source", "track", "wbr",
]);

function checkNesting(source) {
  const stack = [];
  let svgDepth = 0;
  const token = /<(\/?)([a-zA-Z][\w:-]*)([^>]*?)(\/?)>/g;
  let match;
  while ((match = token.exec(source))) {
    const closing = match[1] === "/";
    const tag = match[2].toLowerCase();
    const selfClosing = match[4] === "/";
    const line = source.slice(0, match.index).split("\n").length;

    if (tag === "svg") {
      svgDepth = Math.max(0, svgDepth + (closing ? -1 : 1));
      continue;
    }
    if (svgDepth > 0) continue;
    if (tag === "script" || tag === "style" || voidTags.has(tag) || selfClosing) continue;

    if (!closing) {
      stack.push({ tag, line });
    } else {
      const open = stack.pop();
      if (!open) return { line, message: `unexpected closing </${tag}>` };
      if (open.tag !== tag) {
        return {
          line,
          message: `</${tag}> closes <${open.tag}> opened at line ${open.line}`,
        };
      }
    }
  }
  if (stack.length) {
    const open = stack[stack.length - 1];
    return { line: open.line, message: `<${open.tag}> is never closed` };
  }
  return null;
}

let failed = false;
if (dupes.length) {
  console.error(`duplicate functions: ${dupes.join(", ")}`);
  failed = true;
}
if (missing.length) {
  console.error(`#ids referenced in app.js but missing from index.html: ${missing.join(", ")}`);
  failed = true;
}
if (iconNames.length) {
  console.error(`icons defined but never used: ${iconNames.join(", ")}`);
  failed = true;
}
const nesting = checkNesting(html);
if (nesting) {
  console.error(`index.html:${nesting.line}: ${nesting.message}`);
  failed = true;
}

if (failed) process.exit(1);
console.log(`ui ok — ${names.length} functions, ${referenced.length} element ids, markup balanced`);
