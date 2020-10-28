import React, { useState } from 'react';
import './App.css';

import { Dispatcher, Processor, Eventer } from "wasm";
function App() {
   let dispatcher = Dispatcher.new();
   dispatcher.start().then(
      (eventer: Eventer) => {
         eventer.run();
         let processor = Processor.from(eventer);


         var numberOfTimes = 5;
         var delay = 1000;
         
         setTimeout( async function() {
            await processor.copy().tick();
            processor.copy().send()
         }, delay);
         
      }
   );
   


   return (
      <div className="App" >
         <div>Sum Results: {2}</div>
         <div>Fib Results: {2}</div>
      </div>
   );
}

export default App;
