import { useState } from "react";

import { laad } from "@/lib/api";

function Legenda() {
  return <span>legenda</span>;
}

export function Kaart({ id }: { id: string }) {
  const [open] = useState(false);
  return (
    <div onClick={() => laad(id)}>
      {open && <Legenda />}
    </div>
  );
}
