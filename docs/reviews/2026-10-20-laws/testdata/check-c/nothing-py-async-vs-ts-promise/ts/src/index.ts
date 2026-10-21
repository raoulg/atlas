export class Kernel {
  async stop(): Promise<void> {}
}

async function shut(k: Kernel): Promise<void> {
  await k.stop();
}

export async function closeAll(ks: Kernel[]): Promise<number> {
  for (const k of ks) {
    await shut(k);
  }
  return ks.length;
}
