import { Label, type Order as Parcel } from "./model";

export function ship(order: Parcel): Label {
  return new Label(`${order.reference} (${order.weight} kg)`);
}
