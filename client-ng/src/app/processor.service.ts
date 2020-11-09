import { Injectable } from '@angular/core';
import { Processor } from 'wasm/pkg/wasm';


export class ProcessorService {

  public processor: Processor;

  constructor() { 
    // window['myService'] = this;
    console.log("STarting processor");
    this.processor = Processor.start();
  }
}
