import { createFileRoute } from "@tanstack/react-router";
import { useServerFn } from "@tanstack/react-start";

import { Kaart } from "@/components/Kaart";
import { Bron, Laag } from "@/lib/model";
import { zoek } from "../lib/api";

export const Route = createFileRoute("/kaart")({
  component: KaartScherm,
});

function KaartScherm() {
  const z = useServerFn(zoek);
  const laag = new Laag(new Bron());
  laag.toon();
  return <Kaart id="1" />;
}
