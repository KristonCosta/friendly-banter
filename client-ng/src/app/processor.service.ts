import { Injectable } from '@angular/core';
import { Processor } from 'wasm/pkg/wasm';

@Injectable({
  providedIn: 'root'
})
export class ProcessorService {

  public processor: Processor;

  constructor() { 
    window['myService'] = this;
    this.processor = Processor.start();
  }
}
