import { start } from "repl";
import { Processor } from "wasm";
let processor_inner = Processor.start();

    


export function processor(): Processor {
    return processor_inner;
} 