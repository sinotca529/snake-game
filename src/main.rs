use rand::prelude::*;
use std::io::{stdout, Stdout, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use std::{collections::LinkedList, io::stdin};
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(&self) -> Self {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, PartialOrd)]
struct Size(u16, u16);

#[derive(Copy, Clone, Hash, Eq, PartialEq, PartialOrd)]
struct Coord(u16, u16);

impl Coord {
    fn adjascent(&self, dir: &Direction) -> Self {
        match dir {
            Direction::Up => Self(self.0, self.1 - 1),
            Direction::Down => Self(self.0, self.1 + 1),
            Direction::Left => Self(self.0 - 1, self.1),
            Direction::Right => Self(self.0 + 1, self.1),
        }
    }

    fn rand(min: &Size, max: &Size) -> Self {
        assert!(min <= max);
        let mut rng = rand::thread_rng();
        Self(
            min.0 + (rng.gen::<u16>() % (1 + max.0 - min.0)),
            min.1 + (rng.gen::<u16>() % (1 + max.1 - min.1)),
        )
    }
}

struct SnakeGameLogic {
    field_size: Size,
    /// Body of snake.
    ///  body[0] is the head of the snake.
    ///  body[body.len() - 1] is the tail of the snake.
    body: LinkedList<Coord>,
    pos_feed: Coord,
    dir: Direction,
}

impl SnakeGameLogic {
    fn new(field_size: Size) -> Self {
        Self {
            field_size,
            body: [Coord(4, 2), Coord(3, 2), Coord(2, 2)].into(),
            pos_feed: Coord(10, 10),
            dir: Direction::Right,
        }
    }

    fn is_inner_field(&self, c: &Coord) -> bool {
        let Size(w, h) = self.field_size;
        (1..w - 1).contains(&c.0) && (1..h - 1).contains(&c.1)
    }

    fn score(&self) -> usize {
        self.body.len() - 3
    }

    fn set_dir(&mut self, dir: Direction) {
        if self.dir.opposite() != dir {
            self.dir = dir;
        }
    }

    /// Move head toward the direction.
    /// Return false if game is over.
    fn r#move(&mut self) -> bool {
        let head = self.body.front().unwrap();
        let adj = head.adjascent(&self.dir);

        // Collide with wall.
        if !self.is_inner_field(&adj) {
            return false;
        }

        // Move or Grow
        self.body.push_front(adj);
        if adj == self.pos_feed {
            let max = Size(self.field_size.0 - 2, self.field_size.1 - 2);
            'outer: loop {
                let next_feed_candidate = Coord::rand(&Size(1, 1), &max);
                for p in &self.body {
                    if p == &next_feed_candidate {
                        continue 'outer;
                    }
                }
                self.pos_feed = next_feed_candidate;
                break;
            }
        } else {
            self.body.pop_back();
        }

        // Collidge with body.
        for p in self.body.iter().skip(1) {
            // adj is the head.
            if p == &adj {
                return false;
            }
        }

        true
    }
}

enum SnakeGameEvent {
    ChangeDir(Direction),
    Render,
    Quit,
}

struct SnakeGameControler {
    logic: SnakeGameLogic,
    event_tx: Sender<SnakeGameEvent>,
    event_rx: Receiver<SnakeGameEvent>,
}

impl SnakeGameControler {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            logic: SnakeGameLogic::new(Size(20, 20)),
            event_tx: tx,
            event_rx: rx,
        }
    }

    fn render(&self, stdout: &mut Stdout) {
        let Size(w, h) = self.logic.field_size;

        let mut char_matrix = Vec::new();

        // wall
        let mut wall_v = vec!['-'; w as usize];
        wall_v[0] = '+';
        wall_v[w as usize - 1] = '+';

        let mut wall_h = vec![' '; w as usize];
        wall_h[0] = '|';
        wall_h[w as usize - 1] = '|';

        char_matrix.push(wall_v.clone());
        for _ in 0..h - 2 {
            char_matrix.push(wall_h.clone());
        }
        char_matrix.push(wall_v);

        // head & body
        let mut body = self.logic.body.iter();
        let head_pos = body.next().unwrap();
        let head_char = match self.logic.dir {
            Direction::Up => '^',
            Direction::Down => 'v',
            Direction::Left => '<',
            Direction::Right => '>',
        };
        char_matrix[head_pos.1 as usize][head_pos.0 as usize] = head_char;
        body.for_each(|p| char_matrix[p.1 as usize][p.0 as usize] = 'x');

        // feed
        char_matrix[self.logic.pos_feed.1 as usize][self.logic.pos_feed.0 as usize] = '@';

        // to string
        let s = char_matrix.iter().fold(String::new(), |acc, v| {
            let s: String = v.iter().collect();
            acc + &s + "\r\n"
        });

        // output
        write!(
            stdout,
            "{}score: {}{}{}",
            termion::cursor::Goto(1, 1),
            self.logic.score(),
            termion::cursor::Goto(1, 2),
            s
        )
        .unwrap();
        stdout.flush().unwrap();
    }

    fn run(mut self) {
        let stdin = stdin();
        let mut stdout = stdout().into_raw_mode().unwrap();

        // initialize stdout
        write!(stdout, "{}{}", termion::clear::All, termion::cursor::Hide).unwrap();

        self.render(&mut stdout);

        // render signal
        let tx = self.event_tx.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(150));
            if tx.send(SnakeGameEvent::Render).is_err() {
                break;
            }
        });

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            for c in stdin.keys() {
                let event = c.unwrap();
                use SnakeGameEvent::*;

                let msg = match event {
                    Key::Char('h') => Some(ChangeDir(Direction::Left)),
                    Key::Char('j') => Some(ChangeDir(Direction::Down)),
                    Key::Char('k') => Some(ChangeDir(Direction::Up)),
                    Key::Char('l') => Some(ChangeDir(Direction::Right)),
                    Key::Char('q') => Some(Quit),
                    _ => None,
                };

                if let Some(msg) = msg {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        });

        while let Ok(e) = self.event_rx.recv() {
            use SnakeGameEvent::*;
            match e {
                ChangeDir(d) => {
                    self.logic.set_dir(d);
                }
                Render => {
                    if !self.logic.r#move() {
                        break;
                    }
                    self.render(&mut stdout);
                }
                Quit => {
                    break;
                }
            }
        }

        // finalize stdout
        write!(stdout, "{}", termion::cursor::Show).unwrap();
    }
}

fn main() {
    let game_ctrl = SnakeGameControler::new();
    game_ctrl.run();
}
