// A tiny OpenAI-compatible mock for trying Inkwell without an API key.
//   node scripts/mock-llm-server.js
// Then add a provider with base URL http://127.0.0.1:8787/v1 (no key) and model
// "mock-thinker" or "mock-fast".
const http = require("http");

const PORT = Number(process.env.PORT || 8787);

const ANSWER = `Here are the **key points** from the document:

1. Inkwell renders Markdown locally in Rust — nothing is uploaded to a render service.
2. Requests go straight to the provider you configure, using your own key.
3. \`Ctrl+J\` toggles this assistant; selecting text quotes it for your next question.

| Surface | What it does |
| --- | --- |
| Reader | GFM tables, code, footnotes |
| Assistant | Streaming answers with markdown |
| Source | Raw Markdown via \`Ctrl+U\` |

\`\`\`rust
let rendered = markdown::render(&source);
println!("{} words", rendered.words);
\`\`\`

That was a **mock** answer — no network call left this machine.`;

function sendJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(body),
  });
  res.end(body);
}

function sse(res, payload) {
  res.write(`data: ${JSON.stringify(payload)}\n\n`);
}

const server = http.createServer((req, res) => {
  if (req.method === "GET" && req.url.startsWith("/v1/models")) {
    return sendJson(res, 200, {
      object: "list",
      data: [{ id: "mock-thinker" }, { id: "mock-fast" }],
    });
  }

  if (req.method === "POST" && req.url.startsWith("/v1/chat/completions")) {
    let raw = "";
    req.on("data", (chunk) => (raw += chunk));
    req.on("end", () => {
      let model = "mock-thinker";
      try {
        model = JSON.parse(raw).model || model;
      } catch {
        /* keep default */
      }

      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      });

      const reasoning =
        model === "mock-thinker"
          ? ["Reading the Markdown… ", "checking headings and tables… ", "drafting a short summary."]
          : [];

      const events = [
        ...reasoning.map((text) => ({ choices: [{ delta: { reasoning_content: text } }] })),
        ...ANSWER.split(/(?<=\s)/).map((text) => ({ choices: [{ delta: { content: text } }] })),
      ];

      let index = 0;
      const timer = setInterval(() => {
        if (index >= events.length) {
          clearInterval(timer);
          res.write("data: [DONE]\n\n");
          res.end();
          return;
        }
        sse(res, { id: "mock", model, ...events[index] });
        index += 1;
      }, 18);

      res.on("close", () => clearInterval(timer));
    });
    return;
  }

  sendJson(res, 404, { error: { message: "Not found. Try /v1/models or /v1/chat/completions." } });
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`Mock LLM ready on http://127.0.0.1:${PORT}/v1`);
});
