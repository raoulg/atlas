export type BronLike = {
  lees(id: string): Promise<unknown>;
};

/** Every call goes to the source chosen when the first one is made. */
export const bron: BronLike = {
  async lees(id) {
    return (await kiesBron()).lees(id);
  },
};

async function kiesBron(): Promise<BronLike> {
  if (process.env.NEP) {
    const { NepBron } = await import("./nep-bron");
    return new NepBron();
  } else {
    const { EchteBron } = await import("./echte-bron");
    return new EchteBron();
  }
}

export async function haal(id: string): Promise<unknown> {
  return bron.lees(id);
}
