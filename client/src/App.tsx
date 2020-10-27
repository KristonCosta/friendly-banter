import React, { useState } from 'react';
import './App.css';

import {add_two_ints, fib} from "wasm";
function App() {
   const [sum, setSum] = useState<number>(add_two_ints(10, 20));
   const [fibb, setFib] = useState<number>(fib(10));
   return (
      <div className="App" >
         <div>Sum Results: {sum}</div>
         <div>Fib Results: {fibb}</div>
      </div>
   );
}

export default App;
