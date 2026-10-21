async function settle(): Promise<void> {}

function settleSync(): void {}

export async function main(): Promise<number> {
  await settle();
  settleSync();
  return 0;
}
