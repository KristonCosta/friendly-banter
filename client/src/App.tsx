import React, { useState } from 'react';
import './App.css';

import { start } from "wasm";
function App() {
   start();
   return (
      <div className="App" >
         <div>Sum Results: {2}</div>
         <div>Fib Results: {2}</div>
      </div>
   );
}

export default App;
