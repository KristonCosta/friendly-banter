import { ComponentFixture, TestBed } from '@angular/core/testing';

import { PixiComponent } from './pixi.component';

describe('PixiComponent', () => {
  let component: PixiComponent;
  let fixture: ComponentFixture<PixiComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      declarations: [ PixiComponent ]
    })
    .compileComponents();
  });

  beforeEach(() => {
    fixture = TestBed.createComponent(PixiComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });
});
