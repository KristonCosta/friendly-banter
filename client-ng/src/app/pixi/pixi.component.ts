import { Component, ElementRef, HostListener, Input, NgZone, OnDestroy, OnInit } from '@angular/core';
import { Application, Circle, Graphics, Rectangle, Sprite, Texture } from 'pixi.js';
import { ProcessorService } from '../processor.service';

@Component({
  selector: 'app-pixi',
  templateUrl: './pixi.component.html',
  styleUrls: ['./pixi.component.scss']
})
export class PixiComponent implements OnInit, OnDestroy {

  public app: Application;

  constructor(private elementRef: ElementRef, private ngZone: NgZone, private processor: ProcessorService) {}

  // private entities: Map<number, Sprite> = new Map();
  // private rect: Sprite;
  private graphics: Graphics;
  private vel = 10;

  init() {
    this.ngZone.runOutsideAngular(() => {
      this.app = new Application({
        width: 500, 
        height: 400
      });
      this.graphics = new Graphics();
      this.app.stage.addChild(this.graphics);
      this.app.ticker
    });
    this.elementRef.nativeElement.appendChild(this.app.view);
    this.app.ticker.add(delta => this.tick(delta));
  }

  ngOnInit(): void {
    this.init();
  }

  tick(delta: number): void {
    this.graphics.beginFill(0x288b22);
    this.processor.processor.state().forEach((value, index) => {
      if (value['object_info'].hasOwnProperty('Tree')) {
        let props = value['object_info'].Tree 
        this.graphics.drawCircle(props['position'][0], props['position'][1], props['size']);
      }
    });
    this.graphics.endFill();
  }


  destroy() {
    this.app.destroy();
  }

  ngOnDestroy(): void {
    this.destroy();
  }

}