import React, { useState } from 'react';
import './App.css';
import { ChatForm } from './components/chat_form';
import { ChatMessages } from './components/chat_messages';
import {processor as p} from './services/wasm_runner';

function App() {
   var cnt = 0;
   var interval: number = -1;


   return (
      <div className="App" >
         <div>Sum Results: {2}</div>
         <div>Fib Results: {2}</div>
            <ChatForm/>
            <ChatMessages/>
      </div>
   );
}

export default App;
