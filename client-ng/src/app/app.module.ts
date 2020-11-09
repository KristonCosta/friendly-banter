import { BrowserModule } from '@angular/platform-browser';
import { NgModule, OnInit } from '@angular/core';

import { AppRoutingModule } from './app-routing.module';
import { AppComponent } from './app.component';
import { PixiComponent } from './pixi/pixi.component';
import { ProcessorService } from './processor.service';

@NgModule({
  declarations: [
    AppComponent,
    PixiComponent,
  ],
  imports: [
    BrowserModule,
    AppRoutingModule
  ],
  providers: [
    ProcessorService
  ],
  bootstrap: [AppComponent]
})
export class AppModule {
  
 }
