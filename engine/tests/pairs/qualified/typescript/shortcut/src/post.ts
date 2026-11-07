import * as model from "./model";

export function ship(order: model.Order): model.Label {
  return new model.Label(`${order.reference} (${order.weight} kg)`);
}
