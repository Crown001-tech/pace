import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");

console.log("[pace-extension] Compiling TypeScript...");
execSync("pnpm exec tsc -p tsconfig.json", { cwd: root, stdio: "inherit" });

// Rewrite extensionless relative imports inside dist/ so Chrome ES modules (background & popup) resolve ".js" cleanly
function patchImports(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      patchImports(full);
    } else if (entry.name.endsWith(".js")) {
      const src = fs.readFileSync(full, "utf8");
      const patched = src.replace(
        /(from\s+["'])(\.[^"']+?)(["'])/g,
        (match, p1, p2, p3) => (p2.endsWith(".js") ? match : `${p1}${p2}.js${p3}`)
      );
      if (patched !== src) {
        fs.writeFileSync(full, patched, "utf8");
      }
    }
  }
}

patchImports(path.join(root, "dist"));

// Bundle content script into a single CSP-proof IIFE dist/content.js
function stripModuleSyntax(code) {
  return code
    .replace(/^import\s+[\s\S]*?from\s+["'][^"']+["'];\s*$/gm, "")
    .replace(/^export\s+(?=(async\s+)?function\s+|const\s+|let\s+|class\s+)/gm, "")
    .replace(/^export\s+\{[^}]*\};\s*$/gm, "");
}

const distDir = path.join(root, "dist");
const parts = [
  fs.readFileSync(path.join(distDir, "types.js"), "utf8"),
  fs.readFileSync(path.join(distDir, "content", "flow-adapter.js"), "utf8"),
  fs.readFileSync(path.join(distDir, "content", "queue-runner.js"), "utf8"),
  fs.readFileSync(path.join(distDir, "content", "index.js"), "utf8"),
].map(stripModuleSyntax);

const bundledContent = `(() => {\n"use strict";\n${parts.join("\n\n")}\n})();\n`;
fs.writeFileSync(path.join(distDir, "content.js"), bundledContent, "utf8");

console.log("[pace-extension] Build complete -> apps/extension/dist (including dist/content.js)");
