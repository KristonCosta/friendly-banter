import { Component, ElementRef, HostListener, Input, NgZone, OnDestroy, OnInit } from '@angular/core';
import { Application, Rectangle, Sprite, Texture } from 'pixi.js';
import { ProcessorService } from '../processor.service';

@Component({
  selector: 'app-pixi',
  templateUrl: './pixi.component.html',
  styleUrls: ['./pixi.component.scss']
})
export class PixiComponent implements OnInit, OnDestroy {

  public app: Application;

  constructor(private elementRef: ElementRef, private ngZone: NgZone, private processor: ProcessorService) {}

  private rect: Sprite;
  private vel = 10;

  init() {
    this.ngZone.runOutsideAngular(() => {
      this.app = new Application({
        width: 500, 
        height: 400
      });
      this.rect = Sprite.from(Texture.WHITE);
      this.rect.width = 10;
      this.rect.height = 10;
      this.rect.tint = 0xFF0000;
      this.app.stage.addChild(this.rect);
      this.app.ticker
    });
    this.elementRef.nativeElement.appendChild(this.app.view);
    this.app.ticker.add(delta => this.tick(delta));
  }

  ngOnInit(): void {
    this.init();
  }

  tick(delta: number): void {
    this.rect.x = this.processor.processor.x();
    this.rect.y = this.processor.processor.y();
  }


  destroy() {
    this.app.destroy();
  }

  ngOnDestroy(): void {
    this.destroy();
  }

}