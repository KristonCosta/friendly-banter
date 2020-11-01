import React, { useState } from 'react';
import './App.css';
import {processor as p} from './services/wasm_runner';

function App() {
   var cnt = 0;
   var interval: number = -1;


   return (
      <div className="App" >
         <div>Sum Results: {2}</div>
         <div>Fib Results: {2}</div>
         
      </div>
   );
}

export default App;
