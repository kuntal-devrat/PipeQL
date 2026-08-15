import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const config = vscode.workspace.getConfiguration("pipeql");

  if (config.get<boolean>("lsp.enabled", true)) {
    const serverModule = resolveServerPath(context, config);
    if (serverModule) {
      const serverOptions: ServerOptions = {
        run: { module: serverModule, transport: TransportKind.stdio },
        debug: { module: serverModule, transport: TransportKind.stdio },
      };

      const clientOptions: LanguageClientOptions = {
        documentSelector: [{ language: "pipeql" }],
        synchronize: {
          configurationSection: "pipeql",
        },
      };

      client = new LanguageClient(
        "pipeql",
        "PipeQL Language Server",
        serverOptions,
        clientOptions,
      );
      client.start();
    } else {
      void vscode.window.showWarningMessage(
        "PipeQL LSP binary not found. Install it with `cargo build -p pipeql-lsp` or set pipeql.lsp.path. Syntax highlighting still works.",
      );
    }
  }

  context.subscriptions.push(
    vscode.commands.registerCommand("pipeql.compileToSql", () => {
      void compileActiveDocument();
    }),
  );
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

function resolveServerPath(
  context: vscode.ExtensionContext,
  config: vscode.WorkspaceConfiguration,
): string | undefined {
  const configured = config.get<string>("lsp.path", "");
  if (configured) {
    return configured;
  }
  const bundled = vscode.Uri.joinPath(
    context.extensionUri,
    "bin",
    process.platform === "win32" ? "pipeql-lsp.exe" : "pipeql-lsp",
  );
  if (bundled && bundled.scheme === "file") {
    const fs = require("fs") as typeof import("fs");
    if (fs.existsSync(bundled.fsPath)) {
      return bundled.fsPath;
    }
  }
  const candidates = ["pipeql-lsp"];
  if (process.platform === "win32") {
    candidates.push("pipeql-lsp.exe");
  }
  const childProcess = require("child_process") as typeof import("child_process");
  for (const name of candidates) {
    try {
      const result = childProcess.spawnSync(
        process.platform === "win32" ? "where" : "which",
        [name],
        { encoding: "utf8" },
      );
      if (result.status === 0 && result.stdout.trim()) {
        return result.stdout.trim().split(/\r?\n/)[0];
      }
    } catch {
      // keep searching
    }
  }
  return undefined;
}

async function compileActiveDocument(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "pipeql") {
    return;
  }
  const config = vscode.workspace.getConfiguration("pipeql");
  const dialect = config.get<string>("defaultDialect", "postgres");
  const childProcess = require("child_process") as typeof import("child_process");
  const cli = findCli();
  if (!cli) {
    void vscode.window.showErrorMessage(
      "PipeQL CLI not found. Install it with `cargo install --path crates/pipeql-cli`.",
    );
    return;
  }
  const result = childProcess.spawnSync(
    cli,
    // `--` ends option parsing: query text often starts with a `--` comment.
    ["compile", "--dialect", dialect, "--json", "--", editor.document.getText()],
    { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
  );
  if (result.status !== 0) {
    void vscode.window.showErrorMessage(
      `PipeQL compile failed: ${(result.stderr || result.stdout).trim()}`,
    );
    return;
  }
  let sqlContent = result.stdout;
  try {
    const parsed = JSON.parse(result.stdout);
    if (parsed && typeof parsed.sql === "string") {
      sqlContent = parsed.sql;
    }
  } catch {
    // fallback to raw stdout
  }
  const doc = await vscode.workspace.openTextDocument({
    language: "sql",
    content: sqlContent,
  });
  void vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside, true);
}

function findCli(): string | undefined {
  const config = vscode.workspace.getConfiguration("pipeql");
  const configured = config.get<string>("cliPath", "");
  if (configured) {
    return configured;
  }
  const candidates = ["pipeql", "pipeql.exe"];
  const childProcess = require("child_process") as typeof import("child_process");
  for (const name of candidates) {
    try {
      const result = childProcess.spawnSync(
        process.platform === "win32" ? "where" : "which",
        [name],
        { encoding: "utf8" },
      );
      if (result.status === 0 && result.stdout.trim()) {
        return result.stdout.trim().split(/\r?\n/)[0];
      }
    } catch {
      // keep searching
    }
  }
  return undefined;
}
