import { Component, ElementRef, HostListener, Input, NgZone, OnDestroy, OnInit } from '@angular/core';
import { Application, Circle, Graphics, InteractionEvent, Rectangle, RenderTexture, Sprite, Texture } from 'pixi.js';
import { ProcessorService } from '../processor.service';

const generateCircleTexture = (renderer, color) => {
  const gfx = new Graphics();
  const texture = RenderTexture.create({width: 10, height: 10});

  gfx.beginFill(color);
  gfx.drawCircle(5, 5, 5);
  gfx.endFill();

  renderer.render(gfx, texture);

  return texture;
}

@Component({
  selector: 'app-pixi',
  templateUrl: './pixi.component.html',
  styleUrls: ['./pixi.component.scss']
})
export class PixiComponent implements OnInit, OnDestroy {

  public app: Application;
  public messages: Array<String> = new Array();
  constructor(private elementRef: ElementRef, private ngZone: NgZone, private processor: ProcessorService) {}

  // private entities: Map<number, Sprite> = new Map();
  // private rect: Sprite;
  private graphics: Graphics;
  private texture: any;
  private vel = 10;
  private lookup = new Map<number, Sprite>();
  init() {
    this.ngZone.runOutsideAngular(() => {
      this.app = new Application({
        width: 500, 
        height: 400
      });
      this.texture = generateCircleTexture(this.app.renderer, 0x288b22);
    });
    this.elementRef.nativeElement.appendChild(this.app.view);
    this.app.ticker.add(delta => this.tick(delta));
  }

  ngOnInit(): void {
    this.init();
  }
  

  tick(delta: number): void {
    const messages = this.processor.processor.get_pending();
    if (messages.length > 0) {
      console.log(messages)
    }
    messages.forEach((x) => {this.messages.push(x)});
    
    this.processor.processor.state().forEach((value, index) => {
      const id = value['id'];
      
      if (value['object_info'].hasOwnProperty('Tree')) {
        let props = value['object_info'].Tree 
        const x = props['position'][0];
        const y = props['position'][1];
        if (!this.lookup.has(id)) {
          const sprite = new Sprite(this.texture);
          sprite.scale.x = (10/props['size']);
          sprite.scale.y = (10/props['size']);
          this.lookup.set(id, sprite);
          this.app.stage.addChild(sprite);
          sprite.interactive = true;
          const self = this;
          sprite.on('mouseup', function (event: InteractionEvent) {
            let x = event.data.global.x;
            let y = event.data.global.y;
            self.processor.processor.click(x, y);
          });
          sprite.on('touchstart', function (event: InteractionEvent) {
            let x = event.data.global.x;
            let y = event.data.global.y;
            self.processor.processor.click(x, y);
          });
        }
        let sprite = this.lookup.get(id);
        sprite.position.x = x; 
        sprite.position.y = y;
        

      }
    });
  }



  destroy() {
    this.app.destroy();
  }

  ngOnDestroy(): void {
    this.destroy();
  }

}