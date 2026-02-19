import { registry } from "@/lib/registry";
import { buttonDefinition } from "./ButtonWidget";
import { imageDefinition } from "./ImageWidget";

registry.registerWidget("button", buttonDefinition);
registry.registerWidget("image", imageDefinition);
