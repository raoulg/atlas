import { Label, type Order } from "./model";

export function ship(order: Order): Label {
  return new Label(`${order.reference} (${order.weight} kg)`);
}
