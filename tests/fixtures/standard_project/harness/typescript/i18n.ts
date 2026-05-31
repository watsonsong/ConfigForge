let textCatalog: Map<string, string> | undefined;

export function setGlobalTextCatalog(catalog: Map<string, string>): void {
  textCatalog = catalog;
}

export function globalTranslate(context: string | undefined, msgid: string): string | undefined {
  return textCatalog?.get(context ? `${context}\u0004${msgid}` : msgid);
}
