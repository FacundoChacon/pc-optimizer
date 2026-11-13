import ts from "typescript";

/** Hallazgo de la guardia: un literal de UI o un movimiento sin `motion-safe:`. */
export interface UiLiteralFinding {
  file: string;
  line: number;
  kind: string;
  text: string;
}

/** Atributos cuyo valor se lee en pantalla o con lector de pantalla. */
const TEXT_ATTRIBUTES: ReadonlySet<string> = new Set([
  "aria-label",
  "aria-description",
  "aria-roledescription",
  "aria-valuetext",
  "title",
  "alt",
  "placeholder",
  "label",
]);

function isStringLike(node: ts.Node): boolean {
  return (
    ts.isStringLiteral(node) ||
    ts.isNoSubstitutionTemplateLiteral(node) ||
    ts.isTemplateExpression(node)
  );
}

function isMotionToken(token: string): boolean {
  const parts = token.split(":");
  const base = parts[parts.length - 1] ?? "";
  const variants = parts.slice(0, -1);
  const moves = base === "transition" || base.startsWith("transition-") || base.startsWith("animate-");
  return moves && !variants.includes("motion-safe");
}

function stringLiteralsIn(node: ts.Node): ts.Node[] {
  const found: ts.Node[] = [];
  const visit = (child: ts.Node): void => {
    if (ts.isStringLiteral(child) || ts.isNoSubstitutionTemplateLiteral(child)) found.push(child);
    ts.forEachChild(child, visit);
  };
  visit(node);
  return found;
}

/** Detecta literales de UI y movimiento sin `motion-safe:` en el codigo TSX de `source`. */
export function findUiLiterals(file: string, source: string): UiLiteralFinding[] {
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const findings: UiLiteralFinding[] = [];
  const report = (node: ts.Node, kind: string, text: string): void => {
    const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
    findings.push({ file, line, kind, text });
  };

  const visit = (node: ts.Node): void => {
    if (ts.isJsxText(node) && node.text.trim().length > 0) {
      report(node, "text", node.text.trim());
    } else if (
      ts.isJsxExpression(node) &&
      (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent)) &&
      node.expression !== undefined &&
      isStringLike(node.expression)
    ) {
      report(node, "child-string", node.expression.getText(sf));
    } else if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(sf);
      const init = node.initializer;
      if (TEXT_ATTRIBUTES.has(name) && init !== undefined) {
        const literal =
          ts.isStringLiteral(init) ||
          (ts.isJsxExpression(init) && init.expression !== undefined && isStringLike(init.expression));
        if (literal) report(node, `attr:${name}`, init.getText(sf));
      } else if (name === "className" && init !== undefined) {
        for (const literal of stringLiteralsIn(init)) {
          const text = ts.isStringLiteral(literal) || ts.isNoSubstitutionTemplateLiteral(literal) ? literal.text : "";
          const offending = text.split(/\s+/).filter(isMotionToken);
          if (offending.length > 0) report(literal, "motion", offending.join(" "));
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return findings;
}
