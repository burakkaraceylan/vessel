import { registry } from "@/lib/registry";
import { buttonDefinition } from "./ButtonWidget";

registry.registerWidget("button", buttonDefinition);
console.log(registry);
