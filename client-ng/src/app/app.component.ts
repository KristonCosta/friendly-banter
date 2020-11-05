import { Component, OnInit } from '@angular/core';
import { init_panic_hook, Processor } from 'wasm/pkg/wasm';

@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})
export class AppComponent implements OnInit {

  ngOnInit(): void {
    init_panic_hook();
    Processor.start();
    console.log("Here");
  }

  
  title = 'client-ng';
}
