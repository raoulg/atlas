export interface Row {
  text: string;
}

function addRow(rows: Row[], r: Row): number {
  rows.push(r);
  return rows.length;
}

function addRowGeneric(rows: Array<Row>, r: Row): number {
  rows.push(r);
  return rows.length;
}

function addText(texts: string[], t: string): number {
  texts.push(t);
  return texts.length;
}

export function main(): number {
  return addRow([], { text: "a" }) + addRowGeneric([], { text: "a" }) + addText([], "a");
}
