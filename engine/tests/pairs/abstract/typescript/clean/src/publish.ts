import { CsvExporter, Exporter, JsonExporter } from "./exporters.js";
import { Receipt, Report } from "./reports.js";
import { Shelf, Store, Vault } from "./stores.js";

function exporterFor(format: string): Exporter {
  if (format === "csv") {
    return new CsvExporter();
  }
  return new JsonExporter();
}

function storeFor(place: string): Store {
  if (place === "vault") {
    return new Vault();
  }
  return new Shelf();
}

export function publish(report: Report, format: string): Receipt {
  return exporterFor(format).export(report);
}

export function archive(report: Report, place: string): Receipt {
  return storeFor(place).keep(report);
}
