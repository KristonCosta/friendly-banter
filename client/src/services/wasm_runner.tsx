import { Dispatcher, Processor, Eventer } from "wasm";
let eventer = Eventer.new();
let prom = eventer.run()
let processor_inner = Processor.from(eventer);
processor_inner.send("Hello");
setInterval(async function () {
  await processor_inner.tick();
}, 10);

    


export function processor(): Processor {
    return processor_inner;
} 